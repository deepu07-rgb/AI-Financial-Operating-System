from __future__ import annotations

import hashlib
import json
import tempfile
import subprocess
import sys
import unittest
from pathlib import Path
from uuid import UUID

from PIL import Image, ImageDraw

from aifos_worker.engines import OcrWord
from aifos_worker.main import process_request, render_preview_request, render_report_request
from aifos_worker.preprocessing import preprocess_document
from aifos_worker.protocol import ProcessDocumentRequest, ProtocolError, RenderReportRequest

JOB_ID = "11111111-1111-4111-8111-111111111111"
TOKEN = "22222222-2222-4222-8222-222222222222"


class DeterministicEngine:
    @property
    def identity(self) -> dict[str, str]:
        return {"name": "deterministic-test", "version": "1", "language": "eng"}

    def recognize(self, image_path: Path, page_index: int) -> list[OcrWord]:
        lines = [
            ["Acme", "Supplies", "Ltd"],
            ["Invoice", "No:", "INV-1001"],
            ["Date:", "2026-08-04"],
            ["GSTIN:", "24ABCDE1234F1Z5"],
            ["Subtotal", "100.00"],
            ["Tax", "18.00"],
            ["Total", "118.00"],
            ["Bank", "IFSC", "TEST0000123"],
        ]
        words = []
        for line_index, line in enumerate(lines, 1):
            left = 10
            for text in line:
                width = max(20, len(text) * 8)
                words.append(
                    OcrWord(page_index, text, 0.97, left, line_index * 24, width, 18, 1, 1, line_index)
                )
                left += width + 8
        return words


class WorkerTests(unittest.TestCase):
    def test_worker_entry_denies_network_unapproved_processes_and_external_writes(self) -> None:
        service_root = Path(__file__).resolve().parents[1]
        checks = {
            "network": "import socket; socket.socket()",
            "process": "import subprocess; subprocess.run(['cmd.exe', '/c', 'exit', '0'])",
            "write": "open('outside-worker.txt', 'w', encoding='utf-8')",
        }
        for name, operation in checks.items():
            script = (
                "import sys;"
                f"sys.path.insert(0, {str(service_root)!r});"
                "import worker_entry;"
                f"{operation}"
            )
            completed = subprocess.run(
                [sys.executable, "-I", "-B", "-c", script],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertNotEqual(completed.returncode, 0, name)
            self.assertIn("PermissionError", completed.stderr, name)
    def test_rejects_paths_outside_allowed_root(self) -> None:
        with tempfile.TemporaryDirectory() as allowed, tempfile.TemporaryDirectory() as outside:
            source = Path(outside) / "invoice.png"
            source.write_bytes(b"not-used")
            request = self.request(source, "0" * 64)
            with self.assertRaisesRegex(ProtocolError, "outside the allowed root"):
                ProcessDocumentRequest.parse(request, Path(allowed))

    def test_rejects_unknown_fields_and_protocol_versions(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "invoice.png"
            source.write_bytes(b"not-used")
            request = self.request(source, "0" * 64)
            request["extra"] = True
            with self.assertRaisesRegex(ProtocolError, "missing or unknown"):
                ProcessDocumentRequest.parse(request, Path(directory))
            del request["extra"]
            request["protocol_version"] = 2
            with self.assertRaisesRegex(ProtocolError, "unsupported protocol"):
                ProcessDocumentRequest.parse(request, Path(directory))

    def test_preprocessing_is_reproducible_and_preserves_original(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "invoice.png"
            self.synthetic_invoice(source)
            original = source.read_bytes()
            first = preprocess_document(source, "image/png", root / "first")
            second = preprocess_document(source, "image/png", root / "second")
            self.assertEqual(original, source.read_bytes())
            self.assertEqual(first[0].sha256, second[0].sha256)
            self.assertEqual((first[0].width, first[0].height), (second[0].width, second[0].height))

    def test_structured_result_has_evidence_untrusted_bank_data_and_valid_totals(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "invoice.png"
            self.synthetic_invoice(source)
            digest = hashlib.sha256(source.read_bytes()).hexdigest()
            result = process_request(self.request(source, digest), root, root / "work", DeterministicEngine())
            self.assertEqual(result["status"], "succeeded")
            self.assertEqual(result["document"]["token"], TOKEN)
            self.assertEqual(result["fields"]["invoice_number"]["value_text"], "INV-1001")
            self.assertTrue(result["fields"]["invoice_number"]["sources"])
            self.assertTrue(result["fields"]["bank_details"]["untrusted"])
            self.assertEqual(result["validation"]["status"], "valid")
            json.dumps(result, allow_nan=False)

    def test_render_preview_is_bounded_and_does_not_require_an_ocr_engine(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "invoice.png"
            self.synthetic_invoice(source)
            digest = hashlib.sha256(source.read_bytes()).hexdigest()
            request = self.request(source, digest)
            request["operation"] = "render_preview"
            request["page_index"] = 0
            result = render_preview_request(request, root)
            self.assertEqual(result["status"], "succeeded")
            self.assertEqual(result["page_index"], 0)
            self.assertLessEqual(result["source_width"], 900)
            self.assertTrue(result["data_url"].startswith("data:image/jpeg;base64,"))

    def test_report_protocol_is_confined_and_management_pack_has_five_pages(self) -> None:
        with tempfile.TemporaryDirectory() as directory, tempfile.TemporaryDirectory() as outside:
            root = Path(directory)
            contract_path = root / "contract.json"
            contract_path.write_text(json.dumps(self.report_contract()), encoding="utf-8")
            digest = hashlib.sha256(contract_path.read_bytes()).hexdigest()
            output = root / "management-pack.pdf"
            request = {
                "protocol_version": 1,
                "job_id": JOB_ID,
                "operation": "render_report",
                "report_token": TOKEN,
                "contract_path": str(contract_path.resolve()),
                "expected_sha256": digest,
                "output_path": str(output.resolve()),
                "format": "pdf",
            }
            parsed = RenderReportRequest.parse(request, root, root)
            self.assertEqual(parsed.output_path, output.resolve())
            result = render_report_request(request, root, root)
            self.assertEqual(result["status"], "succeeded")
            self.assertEqual(result["page_count"], 5)
            self.assertTrue(output.read_bytes().startswith(b"%PDF-"))
            request["output_path"] = str((Path(outside) / "escaped.pdf").resolve())
            with self.assertRaisesRegex(ProtocolError, "outside the allowed work root"):
                RenderReportRequest.parse(request, root, root)

    @staticmethod
    def request(source: Path, digest: str) -> dict[str, object]:
        UUID(JOB_ID)
        return {
            "protocol_version": 1,
            "job_id": JOB_ID,
            "operation": "process_document",
            "document_token": TOKEN,
            "source_path": str(source.resolve()),
            "expected_sha256": digest,
            "media_type": "image/png",
            "supplier_templates": [],
        }

    @staticmethod
    def synthetic_invoice(path: Path) -> None:
        image = Image.new("RGB", (900, 1200), "white")
        draw = ImageDraw.Draw(image)
        for index, text in enumerate(["ACME SUPPLIES", "INVOICE INV-1001", "TOTAL 118.00"]):
            draw.text((80, 100 + index * 100), text, fill="black")
        image.save(path, "PNG", compress_level=6)

    @staticmethod
    def report_contract() -> dict[str, object]:
        columns = [
            {"key": "party", "label": "Party"},
            {"key": "total_minor_units", "label": "Amount (minor units)"},
        ]
        table = {
            "id": "upcoming_obligations",
            "title": "Upcoming obligations",
            "columns": columns,
            "rows": [{"party": "Synthetic Supplies", "total_minor_units": 125000}],
            "totals": {"total_minor_units": 125000},
            "source_query_id": "Q-RPT-INVOICES-001",
            "availability_note": "Payment clearance is not connected.",
        }
        return {
            "report_id": "33333333-3333-4333-8333-333333333333",
            "workspace_id": "44444444-4444-4444-8444-444444444444",
            "template_id": "00000000-0000-0000-0000-000000000210",
            "template_version": 1,
            "template_key": "RPT-010",
            "title": "Offline management pack",
            "period_from": "2026-01-01",
            "period_to": "2026-03-31",
            "currency": "INR",
            "filters": {
                "period_from": "2026-01-01",
                "period_to": "2026-03-31",
                "party_id": None,
                "direction": None,
                "status": None,
                "confidence": None,
                "category": None,
            },
            "metrics": [
                {
                    "key": "payables_due",
                    "label": "Approved staged payables",
                    "value_minor_units": 125000,
                    "value_count": None,
                    "currency": "INR",
                    "source_query_id": "Q-RPT-INVOICES-001",
                    "availability_note": "Payment clearance unavailable",
                }
            ],
            "tables": [
                table,
                {**table, "id": "counterparties", "title": "Counterparties"},
                {**table, "id": "exceptions", "title": "Exceptions"},
            ],
            "charts": [
                {
                    "id": "counterparty_spend",
                    "title": "Counterparty spend",
                    "points": [
                        {"label": "Synthetic Supplies", "value": 125000, "currency": "INR"}
                    ],
                    "source_query_id": "Q-RPT-INVOICES-001",
                    "availability_note": None,
                }
            ],
            "data_quality": {
                "document_count": 1,
                "approved_invoice_count": 1,
                "review_pending": 0,
                "payment_source_available": False,
                "cash_source_available": False,
                "category_source_available": False,
                "warnings": ["Cash source is unavailable."],
            },
            "reconciliation": {
                "source_record_count": 1,
                "included_record_count": 1,
                "excluded_record_count": 0,
                "currencies": ["INR"],
                "exclusions": [],
                "missing_sources": ["Cash snapshot unavailable"],
                "generated_by_user_id": "55555555-5555-4555-8555-555555555555",
                "generated_by_name": "Synthetic Reviewer",
                "generated_by_role": "reviewer",
                "period_boundary": "Inclusive issue date 2026-01-01 through 2026-03-31.",
            },
            "source_references": [],
            "executive_summary": ["One open obligation requires attention."],
            "recommended_actions": ["Review open obligations."],
            "further_questions": ["Is settlement evidence complete?"],
            "caveats": ["No foreign-exchange conversion is performed."],
            "boundary_statement": "Management support only; not statutory books.",
            "source_coverage_statement": "One approved operational record and one immutable document.",
            "notes": "Synthetic test note entered by a user.",
            "branding": {
                "company_name": "Synthetic Works",
                "report_title": "Offline management pack",
                "accent_hex": "#153F35",
                "footer_text": "Generated locally - confidential",
            },
            "generated_at": 1785791400000,
            "generator": "offline-report-engine/1",
        }


if __name__ == "__main__":
    unittest.main()
