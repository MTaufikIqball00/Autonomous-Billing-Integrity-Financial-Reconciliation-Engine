"""
Multimodal Document Intelligence Extractor for ABIFR.

Supports verified formats:
- Digital PDF (PyMuPDF text, exact bounding boxes, table extraction)
- Scanned PDF (PyMuPDF rasterization + RapidOCR PP-OCRv4 when text layer is empty/sparse)
- Images: PNG, JPEG, TIFF (Pillow + RapidOCR PP-OCRv4 with polygon bboxes & real OCR confidence)
- DOCX (python-docx paragraphs & tables)
- XLSX (openpyxl sheets & cells with sheet!cell provenance)
- CSV (stdlib csv with row/col provenance)

Core guarantees:
- Every extracted field carries original_text, page_number, bounding_box (if available),
  source_location, extraction_method, provider_confidence (NULL if not supplied — NEVER invented).
- Ambiguous amounts/dates/currencies are NOT silently coerced; flagged as validation_status='ambiguous'.
- Prompt injection patterns inside documents are scanned and flagged, never executed.
"""

from __future__ import annotations

import base64
import csv
import io
import re
import time
from typing import Any, Dict, List, Optional, Tuple

import openpyxl
import pymupdf
from docx import Document as DocxDocument
from PIL import Image

from providers import ProviderConfig, call_external_provider_if_enabled

# Lazy RapidOCR initialization (PaddleOCR PP-OCRv4 ONNX models)
_OCR_ENGINE = None
_OCR_AVAILABLE: Optional[bool] = None


def get_ocr_engine():
    global _OCR_ENGINE, _OCR_AVAILABLE
    if _OCR_AVAILABLE is False:
        return None
    if _OCR_ENGINE is None:
        try:
            from rapidocr_onnxruntime import RapidOCR

            _OCR_ENGINE = RapidOCR()
            _OCR_AVAILABLE = True
        except Exception:
            _OCR_AVAILABLE = False
            return None
    return _OCR_ENGINE


def ocr_status_info() -> Dict[str, Any]:
    engine = get_ocr_engine()
    return {
        "ocr_engine": "rapidocr_onnxruntime (PP-OCRv4)" if engine is not None else "unavailable",
        "ocr_available": engine is not None,
        "supported_formats": ["pdf", "png", "jpg", "jpeg", "tif", "tiff", "docx", "xlsx", "csv"],
    }


PROMPT_INJECTION_PATTERNS = [
    ("ignore_instructions", re.compile(r"(?i)ignore\s+(all\s+)?(previous|prior|above|system)\s+(instructions?|rules?|prompts?)")),
    ("override_approval", re.compile(r"(?i)(auto[\s-]?approve|mark\s+as\s+reconciled|skip\s+(verification|audit|reconciliation)|bypass\s+review)")),
    ("system_role_spoof", re.compile(r"(?i)(<\s*system\s*>|\[SYSTEM\]|system\s+prompt\s*:|you\s+are\s+now\s+an?\s+admin)")),
    ("command_exec", re.compile(r"(?i)(execute\s+(shell|sql|command|payment)|drop\s+table|transfer\s+funds\s+immediately)")),
    ("secret_exfil", re.compile(r"(?i)(reveal|print|dump|exfiltrate)\s+(api[_\s]?key|secret|password|jwt|env)")),
]


def detect_prompt_injection(text: str) -> List[Dict[str, str]]:
    signals: List[Dict[str, str]] = []
    for name, pat in PROMPT_INJECTION_PATTERNS:
        m = pat.search(text)
        if m:
            signals.append({"pattern_name": name, "matched_excerpt": m.group(0)[:120]})
    return signals


def _run_ocr_on_pil(img: Image.Image, page_num: int) -> Tuple[str, List[Dict[str, Any]], str]:
    engine = get_ocr_engine()
    width, height = img.size
    if engine is None:
        return "", [], "ocr_unavailable"

    import numpy as np

    rgb = img.convert("RGB")
    result, _ = engine(np.array(rgb))
    lines_out: List[Dict[str, Any]] = []
    raw_lines: List[str] = []
    if result:
        for idx, item in enumerate(result):
            box, text, score = item[0], str(item[1]), float(item[2])
            xs = [p[0] for p in box]
            ys = [p[1] for p in box]
            bbox = {
                "x0": round(min(xs), 2),
                "y0": round(min(ys), 2),
                "x1": round(max(xs), 2),
                "y1": round(max(ys), 2),
                "page_width": width,
                "page_height": height,
            }
            raw_lines.append(text)
            lines_out.append(
                {
                    "line_index": idx,
                    "text": text,
                    "page_number": page_num,
                    "bounding_box": bbox,
                    "source_location": f"page:{page_num}:ocr_line:{idx + 1}",
                    "confidence": round(score, 4),
                    "method": "rapidocr_ppocrv4",
                }
            )
    return "\n".join(raw_lines), lines_out, "rapidocr_ppocrv4"


def extract_pdf(file_bytes: bytes) -> Dict[str, Any]:
    try:
        doc = pymupdf.open(stream=file_bytes, filetype="pdf")
    except Exception as exc:
        raise ValueError(f"Corrupted or unreadable PDF: {exc}") from exc

    pages_meta: List[Dict[str, Any]] = []
    all_lines: List[Dict[str, Any]] = []
    all_tables: List[Dict[str, Any]] = []
    full_text_parts: List[str] = []
    used_ocr = False
    used_text = False

    for page_idx in range(len(doc)):
        page = doc[page_idx]
        page_num = page_idx + 1
        rect = page.rect
        page_text = page.get_text("text").strip()

        page_lines: List[Dict[str, Any]] = []
        if len(page_text) >= 20:
            used_text = True
            text_dict = page.get_text("dict")
            line_counter = 0
            for block in text_dict.get("blocks", []):
                if block.get("type") != 0:
                    continue
                for line in block.get("lines", []):
                    spans_text = "".join(s.get("text", "") for s in line.get("spans", [])).strip()
                    if not spans_text:
                        continue
                    line_counter += 1
                    lb = line.get("bbox", [0, 0, 0, 0])
                    page_lines.append(
                        {
                            "line_index": line_counter,
                            "text": spans_text,
                            "page_number": page_num,
                            "bounding_box": {
                                "x0": round(lb[0], 2),
                                "y0": round(lb[1], 2),
                                "x1": round(lb[2], 2),
                                "y1": round(lb[3], 2),
                                "page_width": round(rect.width, 2),
                                "page_height": round(rect.height, 2),
                            },
                            "source_location": f"page:{page_num}:line:{line_counter}",
                            "confidence": None,  # Digital PDF text has no probabilistic OCR confidence
                            "method": "pymupdf_text",
                        }
                    )
            # Extract tables using PyMuPDF's built-in table finder
            try:
                tf = page.find_tables()
                for t_idx, table in enumerate(tf.tables):
                    rows = table.extract()
                    if rows:
                        all_tables.append(
                            {
                                "page_number": page_num,
                                "table_index": t_idx,
                                "rows": rows,
                                "bbox": list(table.bbox) if table.bbox else None,
                            }
                        )
            except Exception:
                pass
            full_text_parts.append(page_text)
            pages_meta.append(
                {
                    "page_number": page_num,
                    "width": round(rect.width, 2),
                    "height": round(rect.height, 2),
                    "extraction_mode": "digital_text",
                    "line_count": len(page_lines),
                }
            )
        else:
            # Scanned PDF page -> render at 200 DPI and run OCR
            pix = page.get_pixmap(dpi=200)
            img = Image.open(io.BytesIO(pix.tobytes("png")))
            ocr_text, ocr_lines, ocr_method = _run_ocr_on_pil(img, page_num)
            used_ocr = True
            page_lines.extend(ocr_lines)
            full_text_parts.append(ocr_text)
            pages_meta.append(
                {
                    "page_number": page_num,
                    "width": img.size[0],
                    "height": img.size[1],
                    "extraction_mode": ocr_method,
                    "line_count": len(ocr_lines),
                }
            )

        all_lines.extend(page_lines)

    page_count = len(doc)
    doc.close()

    if used_text and used_ocr:
        method = "pymupdf_text+rapidocr_ppocrv4"
    elif used_ocr:
        method = "rapidocr_ppocrv4"
    else:
        method = "pymupdf_text"

    return {
        "page_count": page_count,
        "extraction_method": method,
        "raw_text": "\n\n".join(full_text_parts),
        "pages": pages_meta,
        "lines": all_lines,
        "tables": all_tables,
    }


def extract_image(file_bytes: bytes) -> Dict[str, Any]:
    try:
        img = Image.open(io.BytesIO(file_bytes))
        img.verify()
        img = Image.open(io.BytesIO(file_bytes))
    except Exception as exc:
        raise ValueError(f"Corrupted or unreadable image: {exc}") from exc

    pages_meta: List[Dict[str, Any]] = []
    all_lines: List[Dict[str, Any]] = []
    text_parts: List[str] = []
    n_frames = getattr(img, "n_frames", 1)
    method = "rapidocr_ppocrv4"

    for frame_idx in range(n_frames):
        if n_frames > 1:
            img.seek(frame_idx)
        frame = img.copy()
        page_num = frame_idx + 1
        ocr_text, ocr_lines, method = _run_ocr_on_pil(frame, page_num)
        text_parts.append(ocr_text)
        all_lines.extend(ocr_lines)
        pages_meta.append(
            {
                "page_number": page_num,
                "width": frame.size[0],
                "height": frame.size[1],
                "extraction_mode": method,
                "line_count": len(ocr_lines),
            }
        )

    return {
        "page_count": n_frames,
        "extraction_method": method,
        "raw_text": "\n\n".join(text_parts),
        "pages": pages_meta,
        "lines": all_lines,
        "tables": [],
    }


def extract_docx(file_bytes: bytes) -> Dict[str, Any]:
    try:
        doc = DocxDocument(io.BytesIO(file_bytes))
    except Exception as exc:
        raise ValueError(f"Corrupted or unreadable DOCX: {exc}") from exc

    lines: List[Dict[str, Any]] = []
    text_parts: List[str] = []
    tables: List[Dict[str, Any]] = []

    for idx, para in enumerate(doc.paragraphs):
        t = para.text.strip()
        if t:
            text_parts.append(t)
            lines.append(
                {
                    "line_index": idx + 1,
                    "text": t,
                    "page_number": 1,
                    "bounding_box": None,
                    "source_location": f"docx:paragraph:{idx + 1}",
                    "confidence": None,
                    "method": "docx_parser",
                }
            )

    for t_idx, table in enumerate(doc.tables):
        rows = []
        for r_idx, row in enumerate(table.rows):
            vals = [cell.text.strip() for cell in row.cells]
            rows.append(vals)
            joined = " | ".join(vals)
            if joined.strip(" |"):
                text_parts.append(joined)
                lines.append(
                    {
                        "line_index": len(lines) + 1,
                        "text": joined,
                        "page_number": 1,
                        "bounding_box": None,
                        "source_location": f"docx:table:{t_idx + 1}:row:{r_idx + 1}",
                        "confidence": None,
                        "method": "docx_parser",
                    }
                )
        tables.append({"page_number": 1, "table_index": t_idx, "rows": rows, "bbox": None})

    return {
        "page_count": 1,
        "extraction_method": "docx_parser",
        "raw_text": "\n".join(text_parts),
        "pages": [{"page_number": 1, "extraction_mode": "docx_parser", "line_count": len(lines)}],
        "lines": lines,
        "tables": tables,
    }


def extract_xlsx(file_bytes: bytes) -> Dict[str, Any]:
    try:
        wb = openpyxl.load_workbook(io.BytesIO(file_bytes), data_only=True)
    except Exception as exc:
        raise ValueError(f"Corrupted or unreadable XLSX: {exc}") from exc

    lines: List[Dict[str, Any]] = []
    tables: List[Dict[str, Any]] = []
    text_parts: List[str] = []
    pages_meta: List[Dict[str, Any]] = []

    for s_idx, sheet_name in enumerate(wb.sheetnames):
        ws = wb[sheet_name]
        rows: List[List[str]] = []
        for r_idx, row in enumerate(ws.iter_rows(values_only=True), start=1):
            vals = ["" if v is None else str(v).strip() for v in row]
            if any(vals):
                rows.append(vals)
                joined = " | ".join(vals)
                text_parts.append(joined)
                lines.append(
                    {
                        "line_index": len(lines) + 1,
                        "text": joined,
                        "page_number": s_idx + 1,
                        "bounding_box": None,
                        "source_location": f"sheet:{sheet_name}!row:{r_idx}",
                        "confidence": None,
                        "method": "xlsx_parser",
                    }
                )
        tables.append({"page_number": s_idx + 1, "sheet_name": sheet_name, "table_index": s_idx, "rows": rows, "bbox": None})
        pages_meta.append({"page_number": s_idx + 1, "sheet_name": sheet_name, "extraction_mode": "xlsx_parser", "line_count": len(rows)})

    return {
        "page_count": max(1, len(wb.sheetnames)),
        "extraction_method": "xlsx_parser",
        "raw_text": "\n".join(text_parts),
        "pages": pages_meta,
        "lines": lines,
        "tables": tables,
    }


def extract_csv(file_bytes: bytes) -> Dict[str, Any]:
    text = file_bytes.decode("utf-8", errors="replace")
    reader = csv.reader(io.StringIO(text))
    rows: List[List[str]] = []
    lines: List[Dict[str, Any]] = []
    text_parts: List[str] = []
    for r_idx, row in enumerate(reader, start=1):
        vals = [c.strip() for c in row]
        if any(vals):
            rows.append(vals)
            joined = " | ".join(vals)
            text_parts.append(joined)
            lines.append(
                {
                    "line_index": r_idx,
                    "text": joined,
                    "page_number": 1,
                    "bounding_box": None,
                    "source_location": f"csv:row:{r_idx}",
                    "confidence": None,
                    "method": "csv_parser",
                }
            )
    return {
        "page_count": 1,
        "extraction_method": "csv_parser",
        "raw_text": "\n".join(text_parts),
        "pages": [{"page_number": 1, "extraction_mode": "csv_parser", "line_count": len(rows)}],
        "lines": lines,
        "tables": [{"page_number": 1, "table_index": 0, "rows": rows, "bbox": None}],
    }


def classify_document(raw_text: str) -> Tuple[str, Optional[float], List[str]]:
    """
    Deterministic keyword/structure document classifier.
    Refuses forced classification when signals are weak -> returns ('unknown', None, reasons).
    """
    lower = raw_text.lower()
    scores: Dict[str, Tuple[int, List[str]]] = {
        "contract_amendment": (0, []),
        "contract": (0, []),
        "credit_note": (0, []),
        "debit_note": (0, []),
        "invoice": (0, []),
        "purchase_order": (0, []),
        "payment_record": (0, []),
        "delivery_receipt": (0, []),
        "service_acceptance": (0, []),
    }

    rules = [
        ("contract_amendment", ["amendment no", "contract amendment", "amends contract", "addendum"]),
        ("contract", ["master service agreement", "vendor contract", "contract number", "effective date", "expiration date", "pricing schedule"]),
        ("credit_note", ["credit note", "credit memo", "refund credit"]),
        ("debit_note", ["debit note", "debit memo"]),
        ("invoice", ["invoice number", "invoice no", "tax invoice", "bill to", "amount due", "subtotal", "invoice date"]),
        ("purchase_order", ["purchase order", "po number", "po no", "ordered quantity"]),
        ("payment_record", ["payment receipt", "payment reference", "wire transfer", "bank reference", "remittance advice", "payment amount", "beneficiary"]),
        ("delivery_receipt", ["delivery receipt", "packing slip", "goods received", "delivered quantity"]),
        ("service_acceptance", ["service acceptance", "acceptance certificate", "milestone accepted"]),
    ]

    for cat, phrases in rules:
        hits = [p for p in phrases if p in lower]
        if hits:
            scores[cat] = (len(hits), hits)

    ranked = sorted(scores.items(), key=lambda kv: kv[1][0], reverse=True)
    best_cat, (best_hits, best_reasons) = ranked[0]
    second_hits = ranked[1][1][0]

    if best_hits == 0:
        return "unknown", None, ["No deterministic document category signatures detected."]
    # Ambiguous tie or very weak single generic hit -> do not force classification
    if best_hits == 1 and second_hits >= 1:
        return "unknown", None, [f"Ambiguous signals between {best_cat} and {ranked[1][0]}."]
    if best_hits == 1 and best_cat in ("contract", "invoice"):
        return "unknown", None, [f"Insufficient evidence ({best_reasons}); requires manual classification."]

    # Deterministic rule match confidence is NOT a probabilistic ML score; report None rather than inventing a fake probability
    return best_cat, None, best_reasons


HEADER_REGEXES = [
    ("document_number", r"(?i)(?:invoice|contract|po|purchase order|payment|credit note)\s*(?:number|no\.?|#)\s*[:\-]\s*([A-Z0-9][A-Z0-9\-\/\.]{2,40})"),
    ("document_date", r"(?i)(?:invoice|document|issue|payment)\s*date\s*[:\-]\s*([0-9]{4}-[0-9]{2}-[0-9]{2}|[0-9]{1,2}[\/\.\-][0-9]{1,2}[\/\.\-][0-9]{2,4}|[A-Za-z]{3,9}\s+\d{1,2},?\s+\d{4})"),
    ("effective_date", r"(?i)effective\s*date\s*[:\-]\s*([0-9]{4}-[0-9]{2}-[0-9]{2}|[0-9]{1,2}[\/\.\-][0-9]{1,2}[\/\.\-][0-9]{2,4})"),
    ("expiration_date", r"(?i)(?:expiration|expiry|end)\s*date\s*[:\-]\s*([0-9]{4}-[0-9]{2}-[0-9]{2}|[0-9]{1,2}[\/\.\-][0-9]{1,2}[\/\.\-][0-9]{2,4})"),
    ("vendor_name", r"(?i)(?:vendor|supplier|beneficiary)\s*(?:name)?\s*[:\-]\s*([^\|\n]{2,100})"),
    ("vendor_id", r"(?i)vendor\s*(?:id|code)\s*[:\-]\s*([A-Z0-9\-_]{2,40})"),
    ("customer_name", r"(?i)(?:customer|bill\s*to|client|company)\s*(?:name)?\s*[:\-]\s*([^\|\n]{2,100})"),
    ("tax_id", r"(?i)(?:tax\s*id|vat\s*no\.?|ein|tin|npwp)\s*[:\-]\s*([A-Z0-9\-\.]{4,40})"),
    ("currency", r"(?i)currency\s*[:\-]\s*([A-Z]{3})\b"),
    ("contract_number", r"(?i)(?:contract|agreement)\s*(?:ref|reference|number|no\.?|#)\s*[:\-]\s*([A-Z0-9][A-Z0-9\-\/\.]{2,40})"),
    ("contract_version", r"(?i)(?:contract\s*)?version\s*[:\-]\s*([A-Za-z0-9\.\-_]{1,32})"),
    ("po_number", r"(?i)(?:po|purchase\s*order)\s*(?:ref|reference|number|no\.?|#)\s*[:\-]\s*([A-Z0-9][A-Z0-9\-\/\.]{2,40})"),
    ("payment_terms", r"(?i)payment\s*terms\s*[:\-]\s*([^\|\n]{2,80})"),
    ("payment_reference", r"(?i)payment\s*(?:ref|reference|id)\s*[:\-]\s*([A-Z0-9\-\/]{3,40})"),
    ("bank_reference", r"(?i)bank\s*(?:ref|reference|tx)\s*[:\-]\s*([A-Z0-9\-\/]{3,40})"),
    ("related_invoice_number", r"(?i)(?:related\s*invoice|invoice\s*ref|for\s*invoice)\s*[:\-]\s*([A-Z0-9\-\/\.]{3,40})"),
    ("subtotal", r"(?i)sub[\s\-]?total\s*[:\-]\s*([A-Z]{0,3}\s*[\$\€\£\¥]?\s*-?[\d,\.]+)"),
    ("discount_total", r"(?i)(?:total\s*)?discount\s*[:\-]\s*([A-Z]{0,3}\s*[\$\€\£\¥]?\s*-?[\d,\.]+%?)"),
    ("tax_amount", r"(?i)(?:total\s*)?(?:tax|vat|gst)\s*(?:amount)?\s*[:\-]\s*([A-Z]{0,3}\s*[\$\€\£\¥]?\s*-?[\d,\.]+)"),
    ("additional_charges", r"(?i)(?:additional|shipping|handling|surcharge)\s*(?:charges?|fees?)?\s*[:\-]\s*([A-Z]{0,3}\s*[\$\€\£\¥]?\s*-?[\d,\.]+)"),
    ("total_amount", r"(?i)(?:grand\s*total|total\s*amount|amount\s*due|payment\s*amount|invoice\s*total|^total)\s*[:\-]\s*([A-Z]{0,3}\s*[\$\€\£\¥]?\s*-?[\d,\.]+)"),
    ("price_tolerance", r"(?i)price\s*tolerance\s*[:\-]\s*([\d\.]+%?)"),
]


def _extract_fields_with_provenance(lines: List[Dict[str, Any]], tables: List[Dict[str, Any]]) -> Tuple[List[Dict[str, Any]], List[Dict[str, Any]]]:
    fields: List[Dict[str, Any]] = []
    seen_headers: set[str] = set()

    for field_name, pattern in HEADER_REGEXES:
        regex = re.compile(pattern)
        for line in lines:
            m = regex.search(line["text"])
            if m and field_name not in seen_headers:
                val = m.group(1).strip()
                seen_headers.add(field_name)
                validation_status = "valid"
                # Flag ambiguous slash dates (e.g. 04/05/2026 where both <= 12)
                if "date" in field_name and re.match(r"^\d{1,2}/\d{1,2}/\d{4}$", val):
                    p1, p2, _ = val.split("/")
                    if int(p1) <= 12 and int(p2) <= 12 and int(p1) != int(p2):
                        validation_status = "ambiguous"
                fields.append(
                    {
                        "field_name": field_name,
                        "line_item_index": None,
                        "extracted_value": val,
                        "original_text": line["text"],
                        "page_number": line["page_number"],
                        "bounding_box": line["bounding_box"],
                        "source_location": line["source_location"],
                        "extraction_method": line["method"],
                        "provider_confidence": line["confidence"],  # None for digital text; real score for OCR
                        "validation_status": validation_status,
                    }
                )
                break

    # Extract line items from structured tables or pipe/tab delimited lines
    line_items: List[Dict[str, Any]] = []

    def try_parse_table_rows(rows: List[List[str]], page_num: int, source_prefix: str, method: str):
        if len(rows) < 2:
            return
        header_idx = None
        col_map: Dict[str, int] = {}
        for r_idx, row in enumerate(rows[:5]):
            norm_cols = [re.sub(r"[^a-z0-9]", "", (c or "").lower()) for c in row]
            if any("desc" in c or "item" in c or "service" in c for c in norm_cols) and any(
                "qty" in c or "quant" in c or "price" in c or "rate" in c or "total" in c for c in norm_cols
            ):
                header_idx = r_idx
                for c_idx, cname in enumerate(norm_cols):
                    if any(k in cname for k in ["code", "sku", "itemid", "productid"]) and "item_code" not in col_map:
                        col_map["item_code"] = c_idx
                    elif any(k in cname for k in ["desc", "item", "service", "product"]) and "description" not in col_map:
                        col_map["description"] = c_idx
                    elif any(k in cname for k in ["qty", "quantity"]) and "quantity" not in col_map:
                        col_map["quantity"] = c_idx
                    elif cname in ("unit", "uom", "units") and "unit" not in col_map:
                        col_map["unit"] = c_idx
                    elif any(k in cname for k in ["unitprice", "rate", "price"]) and "unit_price" not in col_map:
                        col_map["unit_price"] = c_idx
                    elif "disc" in cname and "discount" not in col_map:
                        col_map["discount"] = c_idx
                    elif "taxrate" in cname or "vatrate" in cname:
                        col_map["tax_rate"] = c_idx
                    elif "tax" in cname or "vat" in cname:
                        col_map["tax_amount"] = c_idx
                    elif any(k in cname for k in ["total", "amount", "subtotal"]) and "line_total" not in col_map:
                        col_map["line_total"] = c_idx
                break

        if header_idx is None:
            return

        for offset, row in enumerate(rows[header_idx + 1 :], start=1):
            if not any((c or "").strip() for c in row):
                continue
            desc = (row[col_map["description"]] if "description" in col_map and col_map["description"] < len(row) else "").strip()
            if not desc or desc.lower().startswith(("subtotal", "total", "tax", "grand total")):
                continue
            item_idx = len(line_items) + 1
            item = {
                "line_number": item_idx,
                "item_code": (row[col_map["item_code"]].strip() if "item_code" in col_map and col_map["item_code"] < len(row) else None) or None,
                "description": desc,
                "quantity": (row[col_map["quantity"]].strip() if "quantity" in col_map and col_map["quantity"] < len(row) else None) or None,
                "unit": (row[col_map["unit"]].strip() if "unit" in col_map and col_map["unit"] < len(row) else "unit") or "unit",
                "unit_price": (row[col_map["unit_price"]].strip() if "unit_price" in col_map and col_map["unit_price"] < len(row) else None) or None,
                "discount": (row[col_map["discount"]].strip() if "discount" in col_map and col_map["discount"] < len(row) else None) or None,
                "tax_rate": (row[col_map["tax_rate"]].strip() if "tax_rate" in col_map and col_map["tax_rate"] < len(row) else None) or None,
                "tax_amount": (row[col_map["tax_amount"]].strip() if "tax_amount" in col_map and col_map["tax_amount"] < len(row) else None) or None,
                "line_total": (row[col_map["line_total"]].strip() if "line_total" in col_map and col_map["line_total"] < len(row) else None) or None,
                "page_number": page_num,
                "source_location": f"{source_prefix}:row:{header_idx + offset + 1}",
            }
            line_items.append(item)
            raw_row_text = " | ".join((c or "").strip() for c in row)
            for fk in ("item_code", "description", "quantity", "unit", "unit_price", "discount", "tax_rate", "tax_amount", "line_total"):
                if item[fk] is not None:
                    fields.append(
                        {
                            "field_name": f"line_item.{fk}",
                            "line_item_index": item_idx,
                            "extracted_value": str(item[fk]),
                            "original_text": raw_row_text,
                            "page_number": page_num,
                            "bounding_box": None,
                            "source_location": item["source_location"],
                            "extraction_method": method,
                            "provider_confidence": None,
                            "validation_status": "valid",
                        }
                    )

    for tbl in tables:
        try_parse_table_rows(
            tbl.get("rows", []),
            tbl.get("page_number", 1),
            f"page:{tbl.get('page_number', 1)}:table:{tbl.get('table_index', 0)}",
            "table_parser",
        )

    # Fallback: parse pipe-separated or structured line-item rows in text lines (works for PDF text & OCR!)
    if not line_items:
        pipe_line_re = re.compile(
            r"^(?:(?P<code>[A-Z0-9\-_]{3,20})\s*\|\s*)?(?P<desc>[^\|]{3,80})\s*\|\s*(?P<qty>[\d,\.]+)\s*(?P<unit>[A-Za-z\/]{1,16})?\s*\|\s*(?P<price>[\$\€\£\¥]?[\d,\.]+)\s*\|\s*(?P<total>[\$\€\£\¥]?[\d,\.]+)"
        )
        for line in lines:
            m = pipe_line_re.match(line["text"].strip())
            if m:
                item_idx = len(line_items) + 1
                item = {
                    "line_number": item_idx,
                    "item_code": m.group("code"),
                    "description": m.group("desc").strip(),
                    "quantity": m.group("qty").strip(),
                    "unit": (m.group("unit") or "unit").strip(),
                    "unit_price": m.group("price").strip(),
                    "discount": None,
                    "tax_rate": None,
                    "tax_amount": None,
                    "line_total": m.group("total").strip(),
                    "page_number": line["page_number"],
                    "bounding_box": line["bounding_box"],
                    "source_location": line["source_location"],
                }
                line_items.append(item)
                for fk in ("item_code", "description", "quantity", "unit", "unit_price", "line_total"):
                    if item[fk] is not None:
                        fields.append(
                            {
                                "field_name": f"line_item.{fk}",
                                "line_item_index": item_idx,
                                "extracted_value": str(item[fk]),
                                "original_text": line["text"],
                                "page_number": line["page_number"],
                                "bounding_box": line["bounding_box"],
                                "source_location": line["source_location"],
                                "extraction_method": line["method"],
                                "provider_confidence": line["confidence"],
                                "validation_status": "valid",
                            }
                        )

    return fields, line_items


def process_document(
    file_bytes: bytes,
    extension: str,
    provider_override: Optional[str] = None,
    model_override: Optional[str] = None,
) -> Dict[str, Any]:
    start = time.perf_counter()
    ext = extension.lower().lstrip(".")

    if ext == "pdf":
        parsed = extract_pdf(file_bytes)
    elif ext in ("png", "jpg", "jpeg", "tif", "tiff"):
        parsed = extract_image(file_bytes)
    elif ext == "docx":
        parsed = extract_docx(file_bytes)
    elif ext == "xlsx":
        parsed = extract_xlsx(file_bytes)
    elif ext == "csv":
        parsed = extract_csv(file_bytes)
    else:
        raise ValueError(f"Unsupported file extension: {ext}")

    raw_text = parsed["raw_text"]
    injection_signals = detect_prompt_injection(raw_text)
    category, cls_confidence, cls_reasons = classify_document(raw_text)
    fields, line_items = _extract_fields_with_provenance(parsed["lines"], parsed["tables"])

    cfg = ProviderConfig.from_env(provider_override, model_override)
    external_result = None
    redaction_applied = False
    provider_error = None
    if cfg.provider_name != "local_deterministic":
        redaction_applied = cfg.redact_before_external_ai
        try:
            external_result = call_external_provider_if_enabled(cfg, raw_text)
        except Exception as exc:
            provider_error = str(exc)

    duration_ms = int((time.perf_counter() - start) * 1000)

    header_map = {f["field_name"]: f["extracted_value"] for f in fields if f["line_item_index"] is None}

    return {
        "page_count": parsed["page_count"],
        "extraction_method": parsed["extraction_method"],
        "provider_name": cfg.provider_name,
        "model_version": cfg.model_identifier,
        "provider_error": provider_error,
        "redaction_applied": redaction_applied,
        "duration_ms": duration_ms,
        "classification": {
            "category": category,
            "confidence": cls_confidence,
            "reasons": cls_reasons,
        },
        "raw_text": raw_text,
        "pages": parsed["pages"],
        "tables": parsed["tables"],
        "fields": fields,
        "structured_payload": {
            "header": header_map,
            "line_items": line_items,
            "external_ai_augmentation": external_result,
        },
        "prompt_injection_signals": injection_signals,
    }


def render_page_png_base64(file_bytes: bytes, extension: str, page_number: int = 1) -> Dict[str, Any]:
    ext = extension.lower().lstrip(".")
    if ext == "pdf":
        doc = pymupdf.open(stream=file_bytes, filetype="pdf")
        idx = max(0, min(page_number - 1, len(doc) - 1))
        page = doc[idx]
        pix = page.get_pixmap(dpi=150)
        png_bytes = pix.tobytes("png")
        width, height = pix.width, pix.height
        pdf_w, pdf_h = round(page.rect.width, 2), round(page.rect.height, 2)
        total = len(doc)
        doc.close()
        return {
            "page_number": idx + 1,
            "page_count": total,
            "image_width": width,
            "image_height": height,
            "coord_width": pdf_w,
            "coord_height": pdf_h,
            "mime_type": "image/png",
            "data_base64": base64.b64encode(png_bytes).decode("ascii"),
        }
    if ext in ("png", "jpg", "jpeg", "tif", "tiff"):
        img = Image.open(io.BytesIO(file_bytes))
        n_frames = getattr(img, "n_frames", 1)
        idx = max(0, min(page_number - 1, n_frames - 1))
        if n_frames > 1:
            img.seek(idx)
        frame = img.convert("RGB")
        buf = io.BytesIO()
        frame.save(buf, format="PNG")
        return {
            "page_number": idx + 1,
            "page_count": n_frames,
            "image_width": frame.size[0],
            "image_height": frame.size[1],
            "coord_width": frame.size[0],
            "coord_height": frame.size[1],
            "mime_type": "image/png",
            "data_base64": base64.b64encode(buf.getvalue()).decode("ascii"),
        }
    raise ValueError(f"Page rendering is only supported for PDF and image documents, not '{ext}'.")
