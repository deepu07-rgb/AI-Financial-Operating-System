"""Single-job command-line entry point for the restricted worker."""

from __future__ import annotations

import argparse
import base64
import json
import shutil
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

from . import PROTOCOL_VERSION
from .engines import OcrEngine, TesseractCliEngine
from .extraction import extract_fields, serialize_words
from .preprocessing import (
    preprocess_document,
    preprocessing_manifest,
    render_page_preview,
    sha256_file,
)
from .protocol import (
    MAX_REQUEST_BYTES,
    ProcessDocumentRequest,
    ProtocolError,
    RenderPreviewRequest,
    RenderReportRequest,
)
from .reporting import render_report_pdf, validate_contract


def process_request(
    value: Any, allowed_root: Path, work_root: Path, engine: OcrEngine
) -> dict[str, object]:
    request = ProcessDocumentRequest.parse(value, allowed_root)
    started = time.monotonic()
    actual_sha256 = sha256_file(request.source_path)
    if actual_sha256 != request.expected_sha256:
        raise ProtocolError("managed source hash does not match expected_sha256")
    work_root = work_root.resolve()
    work_root.mkdir(parents=True, exist_ok=True)
    job_directory = Path(tempfile.mkdtemp(prefix=f"{request.job_id}-", dir=work_root))
    try:
        pages = preprocess_document(
            request.source_path,
            request.media_type,
            job_directory / "pages",
        )
        all_words = []
        page_results = []
        for page in pages:
            words = engine.recognize(page.path, page.page_index)
            all_words.extend(words)
            page_results.append(
                {
                    "page_index": page.page_index,
                    "width": page.width,
                    "height": page.height,
                    "words": serialize_words(words),
                }
            )
        fields, warnings, validation = extract_fields(
            all_words, list(request.supplier_templates)
        )
        elapsed_ms = int((time.monotonic() - started) * 1000)
        return {
            "protocol_version": PROTOCOL_VERSION,
            "job_id": request.job_id,
            "status": "succeeded",
            "document": {
                "token": request.document_token,
                "sha256": actual_sha256,
                "media_type": request.media_type,
                "page_count": len(pages),
            },
            "engine": engine.identity,
            "preprocessing": preprocessing_manifest(pages),
            "pages": page_results,
            "fields": fields,
            "line_items": [],
            "validation": validation,
            "warnings": warnings,
            "timing": {"total_ms": elapsed_ms},
        }
    finally:
        shutil.rmtree(job_directory, ignore_errors=True)


def probe(tesseract: Path | None) -> dict[str, object]:
    result: dict[str, object] = {
        "protocol_version": PROTOCOL_VERSION,
        "worker": "aifos-document-worker",
        "python": sys.version.split()[0],
        "ready": False,
    }
    if tesseract is None:
        result["reason"] = "tesseract executable is not configured"
        return result
    try:
        result["engine"] = TesseractCliEngine(tesseract).identity
        result["ready"] = True
    except Exception as error:  # probe must always return machine-readable status
        result["reason"] = str(error)
    return result


def render_preview_request(value: Any, allowed_root: Path) -> dict[str, object]:
    request = RenderPreviewRequest.parse(value, allowed_root)
    actual_sha256 = sha256_file(request.source_path)
    if actual_sha256 != request.expected_sha256:
        raise ProtocolError("managed source hash does not match expected_sha256")
    width, height, preview = render_page_preview(
        request.source_path, request.media_type, request.page_index
    )
    return {
        "protocol_version": PROTOCOL_VERSION,
        "job_id": request.job_id,
        "status": "succeeded",
        "document_token": request.document_token,
        "sha256": actual_sha256,
        "page_index": request.page_index,
        "source_width": width,
        "source_height": height,
        "data_url": "data:image/jpeg;base64,"
        + base64.b64encode(preview).decode("ascii"),
    }


def render_report_request(
    value: Any, allowed_root: Path, work_root: Path
) -> dict[str, object]:
    request = RenderReportRequest.parse(value, allowed_root, work_root)
    actual_sha256 = sha256_file(request.contract_path)
    if actual_sha256 != request.expected_sha256:
        raise ProtocolError("report contract hash does not match expected_sha256")
    with request.contract_path.open("r", encoding="utf-8") as stream:
        contract = validate_contract(json.load(stream))
    page_count = render_report_pdf(contract, request.output_path)
    output_sha256 = sha256_file(request.output_path)
    return {
        "protocol_version": PROTOCOL_VERSION,
        "job_id": request.job_id,
        "status": "succeeded",
        "report_token": request.report_token,
        "contract_sha256": actual_sha256,
        "output_sha256": output_sha256,
        "byte_size": request.output_path.stat().st_size,
        "page_count": page_count,
        "format": "pdf",
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--allowed-root", type=Path)
    parser.add_argument("--work-root", type=Path)
    parser.add_argument("--tesseract", type=Path)
    parser.add_argument("--probe", action="store_true")
    arguments = parser.parse_args()
    if arguments.probe:
        _write(probe(arguments.tesseract))
        return
    if arguments.allowed_root is None:
        parser.error("--allowed-root is required for worker requests")
    raw = sys.stdin.buffer.read(MAX_REQUEST_BYTES + 1)
    if len(raw) > MAX_REQUEST_BYTES:
        _write(_failure(None, "protocol_error", "request exceeds the permitted size"))
        raise SystemExit(2)
    try:
        request_value = json.loads(raw)
        operation = request_value.get("operation") if isinstance(request_value, dict) else None
        if operation == "render_preview":
            response = render_preview_request(request_value, arguments.allowed_root)
        elif operation == "render_report":
            if arguments.work_root is None:
                raise ProtocolError("--work-root is required for report rendering")
            response = render_report_request(
                request_value, arguments.allowed_root, arguments.work_root
            )
        else:
            if arguments.work_root is None:
                raise ProtocolError("--work-root is required for document processing")
            if arguments.tesseract is None:
                raise RuntimeError("tesseract executable is not configured")
            response = process_request(
                request_value,
                arguments.allowed_root,
                arguments.work_root,
                TesseractCliEngine(arguments.tesseract),
            )
    except (json.JSONDecodeError, ProtocolError) as error:
        response = _failure(_job_id_from_raw(raw), "protocol_error", str(error))
    except Exception as error:
        response = _failure(_job_id_from_raw(raw), "processing_error", str(error))
    _write(response)
    if response["status"] != "succeeded":
        raise SystemExit(1)


def _failure(job_id: str | None, code: str, message: str) -> dict[str, object]:
    return {
        "protocol_version": PROTOCOL_VERSION,
        "job_id": job_id,
        "status": "failed",
        "error": {"code": code, "message": message[:2048]},
    }


def _job_id_from_raw(raw: bytes) -> str | None:
    try:
        value = json.loads(raw)
    except (json.JSONDecodeError, UnicodeDecodeError):
        return None
    job_id = value.get("job_id") if isinstance(value, dict) else None
    return job_id if isinstance(job_id, str) else None


def _write(value: dict[str, object]) -> None:
    sys.stdout.write(json.dumps(value, separators=(",", ":"), sort_keys=True))
    sys.stdout.write("\n")


if __name__ == "__main__":
    main()
