"""
Internal Document Intelligence HTTP Worker for ABIFR.

Security & Architecture notes:
- Uses Python standard library ThreadingHTTPServer (no competing web backend / no FastAPI).
- Binds strictly to 127.0.0.1:${DOC_INTEL_PORT} (default 14900).
- Requires `X-Internal-Token` header on all processing endpoints.
- Intended to be called exclusively by the Rust backend.
"""

from __future__ import annotations

import base64
import json
import os
import signal
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any, Dict

from extractor import ocr_status_info, process_document, render_page_png_base64


HOST = os.environ.get("DOC_INTEL_HOST", "127.0.0.1")
PORT = int(os.environ.get("DOC_INTEL_PORT", "14900"))
INTERNAL_TOKEN = os.environ.get("DOC_INTEL_INTERNAL_TOKEN", "")
MAX_BODY_BYTES = 45 * 1024 * 1024  # accommodates base64 overhead for 25MB uploads


class InternalDocIntelHandler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, format: str, *args: Any) -> None:
        sys.stdout.write(f"[doc-intel] {self.address_string()} - {format % args}\n")
        sys.stdout.flush()

    def _send_json(self, status: int, payload: Dict[str, Any]) -> None:
        body = json.dumps(payload).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def _check_auth(self) -> bool:
        if not INTERNAL_TOKEN:
            return True
        provided = self.headers.get("X-Internal-Token", "")
        return provided == INTERNAL_TOKEN

    def _read_json_body(self) -> Dict[str, Any]:
        length = int(self.headers.get("Content-Length", "0"))
        if length <= 0 or length > MAX_BODY_BYTES:
            raise ValueError("Invalid or oversized request body.")
        raw = self.rfile.read(length)
        return json.loads(raw.decode("utf-8"))

    def do_GET(self) -> None:
        if self.path == "/internal/health":
            info = ocr_status_info()
            self._send_json(200, {"status": "ok", "service": "abifr-doc-intel", **info})
            return
        self._send_json(404, {"error": "Not found"})

    def do_POST(self) -> None:
        if not self._check_auth():
            self._send_json(401, {"error": "Unauthorized internal request"})
            return

        try:
            body = self._read_json_body()
        except Exception as exc:
            self._send_json(400, {"error": f"Invalid JSON payload: {exc}"})
            return

        if self.path == "/internal/extract":
            try:
                file_b64 = body.get("file_base64")
                extension = body.get("extension", "")
                if not file_b64 or not extension:
                    self._send_json(400, {"error": "file_base64 and extension are required"})
                    return
                file_bytes = base64.b64decode(file_b64)
                result = process_document(
                    file_bytes=file_bytes,
                    extension=extension,
                    provider_override=body.get("provider"),
                    model_override=body.get("model"),
                )
                self._send_json(200, result)
            except ValueError as exc:
                self._send_json(422, {"error": str(exc)})
            except Exception as exc:
                self._send_json(500, {"error": f"Extraction failed: {exc}"})
            return

        if self.path == "/internal/render-page":
            try:
                file_b64 = body.get("file_base64")
                extension = body.get("extension", "")
                page_number = int(body.get("page_number", 1))
                if not file_b64 or not extension:
                    self._send_json(400, {"error": "file_base64 and extension are required"})
                    return
                file_bytes = base64.b64decode(file_b64)
                rendered = render_page_png_base64(file_bytes, extension, page_number)
                self._send_json(200, rendered)
            except ValueError as exc:
                self._send_json(422, {"error": str(exc)})
            except Exception as exc:
                self._send_json(500, {"error": f"Page render failed: {exc}"})
            return

        self._send_json(404, {"error": "Not found"})


def main() -> None:
    server = ThreadingHTTPServer((HOST, PORT), InternalDocIntelHandler)

    def handle_sig(_signum, _frame):
        sys.stdout.write("[doc-intel] Shutting down cleanly...\n")
        sys.stdout.flush()
        raise SystemExit(0)

    signal.signal(signal.SIGINT, handle_sig)
    signal.signal(signal.SIGTERM, handle_sig)

    sys.stdout.write(f"[doc-intel] Listening internally on http://{HOST}:{PORT}\n")
    sys.stdout.flush()
    try:
        server.serve_forever()
    except (KeyboardInterrupt, SystemExit):
        server.server_close()


if __name__ == "__main__":
    main()
