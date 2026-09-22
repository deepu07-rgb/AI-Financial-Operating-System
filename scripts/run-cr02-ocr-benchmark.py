"""Run the CR-02 packaged OCR corpus and write reproducible evidence reports."""

from __future__ import annotations

import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from statistics import mean, median
from uuid import uuid4

from PIL import Image, ImageDraw, ImageFont, ImageFilter


ROOT = Path(__file__).resolve().parents[1]
RUNTIME = ROOT / "artifacts" / "cr02-runtime-source"
PYTHON = RUNTIME / "python" / "python.exe"
TESSERACT = RUNTIME / "tesseract" / "tesseract.exe"
WORKER = ROOT / "services" / "document-worker" / "worker_entry.py"
REPORT_JSON = ROOT / "docs" / "verification" / "CR-02-OCR-BENCHMARK.json"
REPORT_MD = ROOT / "docs" / "verification" / "CR-02-OCR-BENCHMARK.md"
FIELDS = ("invoice_number", "invoice_date", "supplier", "currency", "total")


def main() -> None:
    for required in (PYTHON, TESSERACT, WORKER):
        if not required.is_file():
            raise SystemExit(f"Required packaged runtime file is missing: {required}")
    with tempfile.TemporaryDirectory(prefix="aifos-cr02-corpus-") as temporary:
        corpus = Path(temporary)
        work = corpus / "worker-jobs"
        work.mkdir()
        cases = build_corpus(corpus)
        results = [run_case(case, corpus, work) for case in cases]
    valid = [result for result in results if result["supported"]]
    failures = [result for result in valid if result["status"] != "succeeded"]
    durations = [result["duration_ms"] for result in valid]
    field_metrics = {}
    for field in FIELDS:
        evaluated = [result for result in valid if result["status"] == "succeeded"]
        correct = sum(result["field_results"].get(field, False) for result in evaluated)
        field_metrics[field] = {
            "correct": correct,
            "evaluated": len(evaluated),
            "accuracy": round(correct / len(evaluated), 4) if evaluated else 0,
        }
    report = {
        "schema_version": 1,
        "generated_at_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "corpus": {
            "name": "AIFOS CR-02 approved synthetic English manufacturing invoice corpus",
            "sample_size": len(results),
            "supported_samples": len(valid),
            "unsupported_samples": len(results) - len(valid),
            "contains_customer_data": False,
        },
        "runtime": {
            "python": version([str(PYTHON), "--version"]),
            "tesseract": version([str(TESSERACT), "--version"]),
            "runtime_source": json.loads((RUNTIME / "runtime-source.json").read_text("utf-8")),
        },
        "hardware": {
            "platform": platform.platform(),
            "processor": platform.processor(),
            "logical_cpu_count": os.cpu_count(),
        },
        "metrics": {
            "field_accuracy": field_metrics,
            "processing_failure_rate": round(len(failures) / len(valid), 4),
            "mean_duration_ms": round(mean(durations), 1),
            "median_duration_ms": round(median(durations), 1),
            "maximum_duration_ms": max(durations),
        },
        "unsupported_cases": [
            "password-protected PDF",
            "handwritten invoice",
            "non-English OCR models",
            "files whose extension and byte signature disagree",
            "documents larger than 250 MiB",
        ],
        "representative_corpus_process": {
            "status": "Founder-controlled process defined; no customer document is included in this repository.",
            "steps": [
                "Obtain written customer consent or use founder-owned invoices.",
                "Remove names, addresses, tax identifiers, bank details, and commercially sensitive line descriptions.",
                "Record source class, scan quality, page count, and expected fields in a separate encrypted register.",
                "Have two finance reviewers approve the ground truth before benchmark inclusion.",
                "Run locally, publish aggregate metrics only, and delete temporary rendered pages.",
            ],
        },
        "results": results,
    }
    REPORT_JSON.parent.mkdir(parents=True, exist_ok=True)
    REPORT_JSON.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    REPORT_MD.write_text(markdown(report), encoding="utf-8")
    if failures:
        raise SystemExit(f"Packaged OCR failed {len(failures)} supported corpus case(s).")
    print(f"CR-02 OCR benchmark passed {len(valid)} supported samples; report: {REPORT_MD}")


def build_corpus(root: Path) -> list[dict[str, object]]:
    cases = []
    suppliers = [
        "ARAVALI STEEL WORKS", "NORTHSTAR FASTENERS", "PRECISION CASTINGS",
        "GREENFIELD PACKAGING", "METRO INDUSTRIAL GASES", "ORBIT MACHINE TOOLS",
        "RIVERSTONE POLYMERS", "SUMMIT ELECTRICALS", "UNITY BEARINGS", "WESTERN LOGISTICS",
    ]
    font_path = Path(os.environ.get("WINDIR", r"C:\Windows")) / "Fonts" / "arial.ttf"
    font = ImageFont.truetype(str(font_path), 52)
    for index, supplier in enumerate(suppliers, start=1):
        expected = {
            "supplier": supplier,
            "invoice_number": f"MFG-{2026000 + index}",
            "invoice_date": f"{index:02d}/08/2026",
            "currency": "INR",
            "total": f"{10000 + index * 1375}.00",
        }
        image = Image.new("RGB", (1800, 2200), "white" if index % 3 else (235, 235, 228))
        draw = ImageDraw.Draw(image)
        lines = [
            supplier,
            "TAX INVOICE",
            f"Invoice No: {expected['invoice_number']}",
            f"Invoice Date: {expected['invoice_date']}",
            "Currency: INR",
            "Description      Qty      Rate      Amount",
            f"Manufacturing material {index}      10      1000.00      10000.00",
            f"Grand Total: INR {expected['total']}",
        ]
        for line_index, line in enumerate(lines):
            draw.text((110, 120 + line_index * 165), line, fill=(15, 20, 18), font=font)
        if index in {4, 8}:
            image = image.filter(ImageFilter.GaussianBlur(radius=0.45))
        suffix = ".jpg" if index % 2 == 0 else ".png"
        path = root / f"synthetic-{index:02d}{suffix}"
        image.save(path, quality=92)
        cases.append({"id": f"synthetic-{index:02d}", "path": path, "expected": expected, "supported": True})
    malformed = root / "malformed-signature.pdf"
    malformed.write_bytes(b"not a pdf")
    cases.append({"id": "malformed-signature", "path": malformed, "expected": {}, "supported": False})
    blank = root / "blank-no-text.png"
    Image.new("RGB", (1000, 1000), "white").save(blank)
    cases.append({"id": "blank-no-text", "path": blank, "expected": {}, "supported": False})
    return cases


def run_case(case: dict[str, object], allowed_root: Path, work_root: Path) -> dict[str, object]:
    path = case["path"]
    assert isinstance(path, Path)
    if not case["supported"]:
        return {"id": case["id"], "supported": False, "status": "documented_unsupported", "duration_ms": 0, "field_results": {}}
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    request = {
        "protocol_version": 1,
        "job_id": str(uuid4()),
        "operation": "process_document",
        "document_token": str(uuid4()),
        "source_path": str(path.resolve()),
        "expected_sha256": digest,
        "media_type": "image/jpeg" if path.suffix == ".jpg" else "image/png",
        "supplier_templates": [],
    }
    started = time.perf_counter()
    completed = subprocess.run(
        [str(PYTHON), "-I", str(WORKER), "--allowed-root", str(allowed_root), "--work-root", str(work_root), "--tesseract", str(TESSERACT)],
        input=json.dumps(request), text=True, capture_output=True, timeout=125, check=False,
        env={"SYSTEMROOT": os.environ.get("SYSTEMROOT", ""), "PATH": "", "PYTHONDONTWRITEBYTECODE": "1", "PYTHONNOUSERSITE": "1"},
    )
    duration_ms = int((time.perf_counter() - started) * 1000)
    try:
        response = json.loads(completed.stdout)
    except json.JSONDecodeError:
        response = {"status": "failed", "error": {"message": completed.stderr[:500]}}
    expected = case["expected"]
    assert isinstance(expected, dict)
    fields = response.get("fields", {}) if response.get("status") == "succeeded" else {}
    field_results = {
        field: normalized(fields.get(field, {}).get("value_text", "")) == normalized(str(value))
        for field, value in expected.items()
    }
    field_values = {
        field: fields.get(field, {}).get("value_text", "") for field in expected
    }
    return {
        "id": case["id"], "supported": True, "status": response.get("status", "failed"),
        "duration_ms": duration_ms, "field_results": field_results,
        "field_values": field_values,
        "warnings": [warning.get("code") for warning in response.get("warnings", [])],
        "diagnostic": response.get("error", {}).get("message"),
    }


def normalized(value: str) -> str:
    return "".join(character.casefold() for character in value if character.isalnum() or character in ".-/")


def version(command: list[str]) -> str:
    return subprocess.run(command, capture_output=True, text=True, check=True).stdout.splitlines()[0].strip()


def markdown(report: dict[str, object]) -> str:
    metrics = report["metrics"]
    assert isinstance(metrics, dict)
    field_accuracy = metrics["field_accuracy"]
    assert isinstance(field_accuracy, dict)
    rows = "\n".join(
        f"| {field.replace('_', ' ').title()} | {value['correct']}/{value['evaluated']} | {value['accuracy']:.1%} |"
        for field, value in field_accuracy.items()
    )
    corpus = report["corpus"]
    hardware = report["hardware"]
    runtime = report["runtime"]
    assert isinstance(corpus, dict) and isinstance(hardware, dict) and isinstance(runtime, dict)
    unsupported = "\n".join(f"- {item}" for item in report["unsupported_cases"])
    return f"""# CR-02 OCR corpus and performance evidence

Generated: {report['generated_at_utc']}

This report uses an approved synthetic corpus only; it contains no customer data. Sample size: **{corpus['sample_size']}** ({corpus['supported_samples']} supported processing samples and {corpus['unsupported_samples']} negative/unsupported samples).

## Runtime and hardware

- Python: {runtime['python']}
- OCR engine: {runtime['tesseract']}
- Platform: {hardware['platform']}
- Processor: {hardware['processor'] or 'not reported by Windows'}
- Logical CPUs: {hardware['logical_cpu_count']}

## Field accuracy

| Field | Correct | Accuracy |
| --- | ---: | ---: |
{rows}

- Processing failure rate: **{metrics['processing_failure_rate']:.1%}**
- Mean processing time: **{metrics['mean_duration_ms']} ms/document**
- Median processing time: **{metrics['median_duration_ms']} ms/document**
- Maximum processing time: **{metrics['maximum_duration_ms']} ms/document**

## Unsupported cases

{unsupported}

## Representative real/anonymized benchmark process

No real customer invoice is committed to the repository. Founder-owned or customer-consented samples must be anonymized, independently ground-truthed by two finance reviewers, run locally, reported only in aggregate, and deleted from temporary processing storage after the run. The exact process is recorded in the JSON evidence file.
"""


if __name__ == "__main__":
    main()
