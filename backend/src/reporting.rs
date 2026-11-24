use crate::{
    error::{AppError, AppResult},
    reconciliation::RECONCILIATION_ENGINE_VERSION,
};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use rust_xlsxwriter::{Format, Workbook};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrencyVarianceBucket {
    pub currency: String,
    pub confirmed_discrepancy_count: usize,
    pub confirmed_variance_total: Decimal,
    pub unresolved_or_potential_count: usize,
    pub unresolved_or_potential_variance_total: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportFindingRow {
    pub finding_id: Uuid,
    pub category: String,
    pub classification_type: String,
    pub severity: String,
    pub status: String,
    pub reviewer_status: String,
    pub vendor_name: Option<String>,
    pub invoice_number: Option<String>,
    pub contract_number: Option<String>,
    pub expected_value: Option<Decimal>,
    pub actual_value: Option<Decimal>,
    pub variance_amount: Option<Decimal>,
    pub currency: Option<String>,
    pub rule_applied: String,
    pub explanation: String,
    pub detected_at: DateTime<Utc>,
    pub evidence_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredAuditReport {
    pub report_type: String,
    pub title: String,
    pub scope: serde_json::Value,
    pub date_from: Option<NaiveDate>,
    pub date_to: Option<NaiveDate>,
    pub documents_analyzed: i64,
    pub invoices_analyzed: i64,
    pub confirmed_discrepancies_count: usize,
    pub potential_anomalies_count: usize,
    pub unresolved_findings_count: usize,
    /// Strictly segregated by currency — NEVER sums across different currencies!
    pub currency_breakdown: Vec<CurrencyVarianceBucket>,
    pub findings: Vec<ReportFindingRow>,
    pub rule_version: String,
    pub model_version: String,
    pub generated_at: DateTime<Utc>,
}

pub async fn build_audit_report(
    db: &PgPool,
    report_type: &str,
    title: &str,
    vendor_id: Option<Uuid>,
    invoice_id: Option<Uuid>,
    contract_id: Option<Uuid>,
    date_from: Option<NaiveDate>,
    date_to: Option<NaiveDate>,
) -> AppResult<StructuredAuditReport> {
    let docs_analyzed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM documents WHERE ($1::uuid IS NULL OR vendor_id = $1)",
    )
    .bind(vendor_id)
    .fetch_one(db)
    .await?;

    let invs_analyzed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM invoices WHERE ($1::uuid IS NULL OR vendor_id = $1) AND ($2::uuid IS NULL OR id = $2)",
    )
    .bind(vendor_id)
    .bind(invoice_id)
    .fetch_one(db)
    .await?;

    let rows = sqlx::query(
        r#"
        SELECT f.id, f.category, f.classification_type, f.severity, f.status, f.reviewer_status,
               v.legal_name AS vendor_name, i.invoice_number, c.contract_number,
               f.expected_value, f.actual_value, f.variance_amount, f.currency,
               f.rule_applied, f.explanation, f.detected_at,
               (SELECT COUNT(*) FROM evidence_references er WHERE er.finding_id = f.id) AS evidence_count
        FROM anomaly_findings f
        LEFT JOIN vendors v ON v.id = f.vendor_id
        LEFT JOIN invoices i ON i.id = f.invoice_id
        LEFT JOIN contracts c ON c.id = f.contract_id
        WHERE ($1::uuid IS NULL OR f.vendor_id = $1)
          AND ($2::uuid IS NULL OR f.invoice_id = $2)
          AND ($3::uuid IS NULL OR f.contract_id = $3)
          AND ($4::date IS NULL OR f.detected_at::date >= $4)
          AND ($5::date IS NULL OR f.detected_at::date <= $5)
          AND (
            $6 <> 'duplicate' OR f.category IN ('DUPLICATE_INVOICE', 'DUPLICATE_PAYMENT_CANDIDATE')
          )
          AND (
            $6 <> 'payment' OR f.category IN ('PAYMENT_MISMATCH', 'DUPLICATE_PAYMENT_CANDIDATE')
          )
        ORDER BY f.detected_at DESC
        "#,
    )
    .bind(vendor_id)
    .bind(invoice_id)
    .bind(contract_id)
    .bind(date_from)
    .bind(date_to)
    .bind(report_type)
    .fetch_all(db)
    .await?;

    let mut findings = Vec::new();
    let mut buckets: BTreeMap<String, CurrencyVarianceBucket> = BTreeMap::new();
    let mut confirmed_count = 0usize;
    let mut potential_count = 0usize;
    let mut unresolved_count = 0usize;

    for r in rows {
        let classification_type: String = r.get("classification_type");
        let currency: Option<String> = r.get("currency");
        let variance: Option<Decimal> = r.get("variance_amount");

        let is_confirmed = classification_type == "confirmed_rule_violation"
            || classification_type == "exact_duplicate";
        let is_potential = classification_type == "potential_anomaly"
            || classification_type == "probable_duplicate";

        if is_confirmed {
            confirmed_count += 1;
        } else if is_potential {
            potential_count += 1;
        } else {
            unresolved_count += 1;
        }

        if let Some(curr) = &currency {
            let entry = buckets
                .entry(curr.to_ascii_uppercase())
                .or_insert_with(|| CurrencyVarianceBucket {
                    currency: curr.to_ascii_uppercase(),
                    confirmed_discrepancy_count: 0,
                    confirmed_variance_total: Decimal::ZERO,
                    unresolved_or_potential_count: 0,
                    unresolved_or_potential_variance_total: Decimal::ZERO,
                });
            let abs_v = variance.unwrap_or(Decimal::ZERO).abs();
            if is_confirmed {
                entry.confirmed_discrepancy_count += 1;
                entry.confirmed_variance_total += abs_v;
            } else {
                entry.unresolved_or_potential_count += 1;
                entry.unresolved_or_potential_variance_total += abs_v;
            }
        }

        findings.push(ReportFindingRow {
            finding_id: r.get("id"),
            category: r.get("category"),
            classification_type,
            severity: r.get("severity"),
            status: r.get("status"),
            reviewer_status: r.get("reviewer_status"),
            vendor_name: r.get("vendor_name"),
            invoice_number: r.get("invoice_number"),
            contract_number: r.get("contract_number"),
            expected_value: r.get("expected_value"),
            actual_value: r.get("actual_value"),
            variance_amount: variance,
            currency,
            rule_applied: r.get("rule_applied"),
            explanation: r.get("explanation"),
            detected_at: r.get("detected_at"),
            evidence_count: r.get("evidence_count"),
        });
    }

    let model_version: String = sqlx::query_scalar(
        "SELECT model_identifier FROM model_metadata WHERE is_default = TRUE LIMIT 1",
    )
    .fetch_optional(db)
    .await?
    .unwrap_or_else(|| "local_deterministic".into());

    Ok(StructuredAuditReport {
        report_type: report_type.to_string(),
        title: title.to_string(),
        scope: serde_json::json!({
            "vendor_id": vendor_id,
            "invoice_id": invoice_id,
            "contract_id": contract_id
        }),
        date_from,
        date_to,
        documents_analyzed: docs_analyzed,
        invoices_analyzed: invs_analyzed,
        confirmed_discrepancies_count: confirmed_count,
        potential_anomalies_count: potential_count,
        unresolved_findings_count: unresolved_count,
        currency_breakdown: buckets.into_values().collect(),
        findings,
        rule_version: RECONCILIATION_ENGINE_VERSION.into(),
        model_version,
        generated_at: Utc::now(),
    })
}

pub fn export_report_csv(report: &StructuredAuditReport) -> AppResult<Vec<u8>> {
    let mut wtr = csv::Writer::from_writer(vec![]);
    wtr.write_record([
        "finding_id",
        "category",
        "classification_type",
        "severity",
        "status",
        "reviewer_status",
        "vendor",
        "invoice_number",
        "contract_number",
        "expected_value",
        "actual_value",
        "variance_amount",
        "currency",
        "rule_applied",
        "evidence_count",
        "detected_at",
        "explanation",
    ])
    .map_err(|e| AppError::Internal(format!("CSV header error: {}", e)))?;

    for f in &report.findings {
        wtr.write_record([
            f.finding_id.to_string(),
            f.category.clone(),
            f.classification_type.clone(),
            f.severity.clone(),
            f.status.clone(),
            f.reviewer_status.clone(),
            f.vendor_name.clone().unwrap_or_default(),
            f.invoice_number.clone().unwrap_or_default(),
            f.contract_number.clone().unwrap_or_default(),
            f.expected_value.map(|v| v.to_string()).unwrap_or_default(),
            f.actual_value.map(|v| v.to_string()).unwrap_or_default(),
            f.variance_amount.map(|v| v.to_string()).unwrap_or_default(),
            f.currency.clone().unwrap_or_default(),
            f.rule_applied.clone(),
            f.evidence_count.to_string(),
            f.detected_at.to_rfc3339(),
            f.explanation.clone(),
        ])
        .map_err(|e| AppError::Internal(format!("CSV write error: {}", e)))?;
    }

    wtr.into_inner()
        .map_err(|e| AppError::Internal(format!("CSV flush error: {}", e)))
}

pub fn export_report_xlsx(report: &StructuredAuditReport) -> AppResult<Vec<u8>> {
    let mut workbook = Workbook::new();
    let bold = Format::new().set_bold();

    // Sheet 1: Executive Summary & Currency Breakdown
    let summary = workbook.add_worksheet();
    summary
        .set_name("Audit Summary")
        .map_err(|e| AppError::Internal(e.to_string()))?;
    summary.write_string_with_format(0, 0, "ABIFR Financial Audit Report", &bold).ok();
    summary.write_string(1, 0, "Title").ok();
    summary.write_string(1, 1, &report.title).ok();
    summary.write_string(2, 0, "Report Type").ok();
    summary.write_string(2, 1, &report.report_type).ok();
    summary.write_string(3, 0, "Rule Engine Version").ok();
    summary.write_string(3, 1, &report.rule_version).ok();
    summary.write_string(4, 0, "Model Version").ok();
    summary.write_string(4, 1, &report.model_version).ok();
    summary.write_string(5, 0, "Generated At (UTC)").ok();
    summary.write_string(5, 1, &report.generated_at.to_rfc3339()).ok();
    summary.write_string(6, 0, "Documents Analyzed").ok();
    summary.write_number(6, 1, report.documents_analyzed as f64).ok();
    summary.write_string(7, 0, "Confirmed Discrepancies").ok();
    summary.write_number(7, 1, report.confirmed_discrepancies_count as f64).ok();
    summary.write_string(8, 0, "Potential Anomalies").ok();
    summary.write_number(8, 1, report.potential_anomalies_count as f64).ok();
    summary.write_string(9, 0, "Unresolved Findings").ok();
    summary.write_number(9, 1, report.unresolved_findings_count as f64).ok();

    summary.write_string_with_format(11, 0, "Currency Breakdown (Never Combined Across Currencies)", &bold).ok();
    for (c, h) in [
        "Currency",
        "Confirmed Count",
        "Confirmed Variance Total",
        "Potential/Unresolved Count",
        "Potential/Unresolved Variance Total",
    ]
    .iter()
    .enumerate()
    {
        summary.write_string_with_format(12, c as u16, *h, &bold).ok();
    }
    for (idx, b) in report.currency_breakdown.iter().enumerate() {
        let r = 13 + idx as u32;
        summary.write_string(r, 0, &b.currency).ok();
        summary.write_number(r, 1, b.confirmed_discrepancy_count as f64).ok();
        summary.write_string(r, 2, &b.confirmed_variance_total.to_string()).ok();
        summary.write_number(r, 3, b.unresolved_or_potential_count as f64).ok();
        summary.write_string(r, 4, &b.unresolved_or_potential_variance_total.to_string()).ok();
    }

    // Sheet 2: Detailed Findings
    let sheet = workbook.add_worksheet();
    sheet
        .set_name("Findings")
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let headers = [
        "Finding ID",
        "Category",
        "Classification",
        "Severity",
        "Status",
        "Reviewer Status",
        "Vendor",
        "Invoice #",
        "Contract #",
        "Expected",
        "Actual",
        "Variance",
        "Currency",
        "Rule Applied",
        "Evidence Refs",
        "Explanation",
    ];
    for (c, h) in headers.iter().enumerate() {
        sheet.write_string_with_format(0, c as u16, *h, &bold).ok();
    }
    for (i, f) in report.findings.iter().enumerate() {
        let r = (i + 1) as u32;
        sheet.write_string(r, 0, &f.finding_id.to_string()).ok();
        sheet.write_string(r, 1, &f.category).ok();
        sheet.write_string(r, 2, &f.classification_type).ok();
        sheet.write_string(r, 3, &f.severity).ok();
        sheet.write_string(r, 4, &f.status).ok();
        sheet.write_string(r, 5, &f.reviewer_status).ok();
        sheet.write_string(r, 6, f.vendor_name.as_deref().unwrap_or("")).ok();
        sheet.write_string(r, 7, f.invoice_number.as_deref().unwrap_or("")).ok();
        sheet.write_string(r, 8, f.contract_number.as_deref().unwrap_or("")).ok();
        sheet.write_string(r, 9, &f.expected_value.map(|v| v.to_string()).unwrap_or_default()).ok();
        sheet.write_string(r, 10, &f.actual_value.map(|v| v.to_string()).unwrap_or_default()).ok();
        sheet.write_string(r, 11, &f.variance_amount.map(|v| v.to_string()).unwrap_or_default()).ok();
        sheet.write_string(r, 12, f.currency.as_deref().unwrap_or("")).ok();
        sheet.write_string(r, 13, &f.rule_applied).ok();
        sheet.write_number(r, 14, f.evidence_count as f64).ok();
        sheet.write_string(r, 15, &f.explanation).ok();
    }

    workbook
        .save_to_buffer()
        .map_err(|e| AppError::Internal(format!("XLSX generation failed: {}", e)))
}

/// Pure-Rust deterministic PDF 1.4 audit report generator (valid xref & catalog, Helvetica font)
pub fn export_report_pdf(report: &StructuredAuditReport) -> Vec<u8> {
    let mut lines: Vec<String> = vec![
        "AGENTIC BILLING INTEGRITY & FINANCIAL RECONCILIATION (ABIFR)".into(),
        format!("AUDIT REPORT: {}", report.title),
        format!("Type: {} | Generated: {}", report.report_type, report.generated_at.format("%Y-%m-%d %H:%M:%S UTC")),
        format!("Rule Engine: {} | Model: {}", report.rule_version, report.model_version),
        format!(
            "Documents: {} | Invoices: {} | Confirmed: {} | Potential: {} | Unresolved: {}",
            report.documents_analyzed,
            report.invoices_analyzed,
            report.confirmed_discrepancies_count,
            report.potential_anomalies_count,
            report.unresolved_findings_count
        ),
        "--------------------------------------------------------------------------------".into(),
        "CURRENCY BREAKDOWN (SEGREGATED - NO UNVERIFIED FX COMBINATION):".into(),
    ];

    if report.currency_breakdown.is_empty() {
        lines.push("  (No currency variances recorded in scope)".into());
    } else {
        for b in &report.currency_breakdown {
            lines.push(format!(
                "  {} | Confirmed: {} (Total Variance: {}) | Potential/Unresolved: {} (Total: {})",
                b.currency,
                b.confirmed_discrepancy_count,
                b.confirmed_variance_total,
                b.unresolved_or_potential_count,
                b.unresolved_or_potential_variance_total
            ));
        }
    }

    lines.push("--------------------------------------------------------------------------------".into());
    lines.push("FINDINGS:".into());
    for (idx, f) in report.findings.iter().take(40).enumerate() {
        let summary = format!(
            "{}. [{}][{}][{}] Inv:{} Var:{} {} Rule:{}",
            idx + 1,
            f.severity.to_uppercase(),
            f.classification_type,
            f.category,
            f.invoice_number.as_deref().unwrap_or("N/A"),
            f.variance_amount.map(|v| v.to_string()).unwrap_or_else(|| "N/A".into()),
            f.currency.as_deref().unwrap_or(""),
            f.rule_applied
        );
        lines.push(summary);
        let exp: String = f.explanation.chars().take(105).collect();
        lines.push(format!("   {}", exp));
    }

    build_minimal_pdf_from_lines(&lines)
}

fn pdf_escape(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii() && !c.is_control())
        .map(|c| match c {
            '\\' => "\\\\".to_string(),
            '(' => "\\(".to_string(),
            ')' => "\\)".to_string(),
            other => other.to_string(),
        })
        .collect()
}

pub fn build_minimal_pdf_from_lines(lines: &[String]) -> Vec<u8> {
    let lines_per_page = 52;
    let chunks: Vec<&[String]> = if lines.is_empty() {
        vec![&[]]
    } else {
        lines.chunks(lines_per_page).collect()
    };

    let mut objects: Vec<String> = Vec::new();
    // Obj 1: Catalog, Obj 2: Pages, Obj 3: Font
    objects.push("1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n".into());

    let mut page_obj_ids = Vec::new();
    let mut next_id = 4usize;
    let mut page_and_stream_objs: Vec<String> = Vec::new();

    for chunk in &chunks {
        let page_id = next_id;
        let content_id = next_id + 1;
        next_id += 2;
        page_obj_ids.push(page_id);

        let mut stream = String::from("BT\n/F1 9 Tf\n40 760 Td\n13 TL\n");
        for (i, line) in chunk.iter().enumerate() {
            if i == 0 {
                stream.push_str(&format!("({}) Tj\n", pdf_escape(line)));
            } else {
                stream.push_str(&format!("T* ({}) Tj\n", pdf_escape(line)));
            }
        }
        stream.push_str("ET\n");

        page_and_stream_objs.push(format!(
            "{} 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>\nendobj\n",
            page_id, content_id
        ));
        page_and_stream_objs.push(format!(
            "{} 0 obj\n<< /Length {} >>\nstream\n{}endstream\nendobj\n",
            content_id,
            stream.len(),
            stream
        ));
    }

    let kids: String = page_obj_ids
        .iter()
        .map(|id| format!("{} 0 R", id))
        .collect::<Vec<_>>()
        .join(" ");
    objects.push(format!(
        "2 0 obj\n<< /Type /Pages /Kids [ {} ] /Count {} >>\nendobj\n",
        kids,
        page_obj_ids.len()
    ));
    objects.push(
        "3 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>\nendobj\n".into(),
    );
    objects.extend(page_and_stream_objs);

    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n");
    let mut offsets = vec![0usize];
    for obj in &objects {
        offsets.push(out.len());
        out.extend_from_slice(obj.as_bytes());
    }
    let xref_pos = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", offsets.len()).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for off in offsets.iter().skip(1) {
        out.extend_from_slice(format!("{:010} 00000 n \n", off).as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            offsets.len(),
            xref_pos
        )
        .as_bytes(),
    );
    out
}
