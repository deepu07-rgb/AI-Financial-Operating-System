"""Strict versioned request/response contracts for the worker boundary."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any
from uuid import UUID

from . import PROTOCOL_VERSION

MAX_REQUEST_BYTES = 64 * 1024
SUPPORTED_MEDIA_TYPES = {"application/pdf", "image/jpeg", "image/png"}
REQUEST_KEYS = {
    "protocol_version",
    "job_id",
    "operation",
    "document_token",
    "source_path",
    "expected_sha256",
    "media_type",
    "supplier_templates",
}
PREVIEW_REQUEST_KEYS = REQUEST_KEYS | {"page_index"}
REPORT_REQUEST_KEYS = {
    "protocol_version",
    "job_id",
    "operation",
    "report_token",
    "contract_path",
    "expected_sha256",
    "output_path",
    "format",
}


class ProtocolError(ValueError):
    """Raised when an untrusted protocol message violates its contract."""


@dataclass(frozen=True)
class ProcessDocumentRequest:
    protocol_version: int
    job_id: str
    operation: str
    document_token: str
    source_path: Path
    expected_sha256: str
    media_type: str
    supplier_templates: tuple[dict[str, str], ...]

    @classmethod
    def parse(cls, value: Any, allowed_root: Path) -> "ProcessDocumentRequest":
        if not isinstance(value, dict) or set(value) != REQUEST_KEYS:
            raise ProtocolError("request has missing or unknown fields")
        if value["protocol_version"] != PROTOCOL_VERSION:
            raise ProtocolError("unsupported protocol version")
        if value["operation"] != "process_document":
            raise ProtocolError("unsupported operation")
        job_id = _uuid(value["job_id"], "job_id")
        token = _uuid(value["document_token"], "document_token")
        expected_sha256 = value["expected_sha256"]
        if not isinstance(expected_sha256, str) or len(expected_sha256) != 64:
            raise ProtocolError("expected_sha256 must be 64 lowercase hexadecimal characters")
        if any(character not in "0123456789abcdef" for character in expected_sha256):
            raise ProtocolError("expected_sha256 must be 64 lowercase hexadecimal characters")
        media_type = value["media_type"]
        if media_type not in SUPPORTED_MEDIA_TYPES:
            raise ProtocolError("unsupported media type")
        templates = value["supplier_templates"]
        if not isinstance(templates, list) or len(templates) > 100:
            raise ProtocolError("supplier_templates must be an array of at most 100 items")
        validated_templates = []
        for template in templates:
            if not isinstance(template, dict) or set(template) != {
                "supplier_key", "field_name", "label_text", "version_id"
            }:
                raise ProtocolError("supplier template has missing or unknown fields")
            if template["field_name"] not in {
                "invoice_number", "invoice_date", "supplier_name", "total_amount"
            }:
                raise ProtocolError("supplier template field is unsupported")
            if not all(isinstance(template[name], str) for name in template):
                raise ProtocolError("supplier template values must be strings")
            if not 2 <= len(template["label_text"].strip()) <= 80:
                raise ProtocolError("supplier template label has an invalid length")
            validated_templates.append({name: template[name].strip() for name in template})
        source_value = value["source_path"]
        if not isinstance(source_value, str) or not source_value:
            raise ProtocolError("source_path must be a non-empty string")
        source_path = Path(source_value)
        if not source_path.is_absolute():
            raise ProtocolError("source_path must be absolute")
        root = allowed_root.resolve(strict=True)
        source = source_path.resolve(strict=True)
        try:
            source.relative_to(root)
        except ValueError as error:
            raise ProtocolError("source_path is outside the allowed root") from error
        if not source.is_file():
            raise ProtocolError("source_path is not a regular file")
        return cls(
            protocol_version=PROTOCOL_VERSION,
            job_id=job_id,
            operation="process_document",
            document_token=token,
            source_path=source,
            expected_sha256=expected_sha256,
            media_type=media_type,
            supplier_templates=tuple(validated_templates),
        )


@dataclass(frozen=True)
class RenderPreviewRequest:
    protocol_version: int
    job_id: str
    operation: str
    document_token: str
    source_path: Path
    expected_sha256: str
    media_type: str
    page_index: int

    @classmethod
    def parse(cls, value: Any, allowed_root: Path) -> "RenderPreviewRequest":
        if not isinstance(value, dict) or set(value) != PREVIEW_REQUEST_KEYS:
            raise ProtocolError("preview request has missing or unknown fields")
        if value.get("operation") != "render_preview":
            raise ProtocolError("unsupported preview operation")
        process_shape = {key: item for key, item in value.items() if key != "page_index"}
        process_shape["operation"] = "process_document"
        process_shape["supplier_templates"] = []
        common = ProcessDocumentRequest.parse(process_shape, allowed_root)
        page_index = value["page_index"]
        if not isinstance(page_index, int) or isinstance(page_index, bool) or page_index < 0:
            raise ProtocolError("page_index must be a non-negative integer")
        return cls(
            protocol_version=common.protocol_version,
            job_id=common.job_id,
            operation="render_preview",
            document_token=common.document_token,
            source_path=common.source_path,
            expected_sha256=common.expected_sha256,
            media_type=common.media_type,
            page_index=page_index,
        )


@dataclass(frozen=True)
class RenderReportRequest:
    protocol_version: int
    job_id: str
    operation: str
    report_token: str
    contract_path: Path
    expected_sha256: str
    output_path: Path
    format: str

    @classmethod
    def parse(
        cls, value: Any, allowed_root: Path, work_root: Path
    ) -> "RenderReportRequest":
        if not isinstance(value, dict) or set(value) != REPORT_REQUEST_KEYS:
            raise ProtocolError("report request has missing or unknown fields")
        if value["protocol_version"] != PROTOCOL_VERSION:
            raise ProtocolError("unsupported protocol version")
        if value["operation"] != "render_report" or value["format"] != "pdf":
            raise ProtocolError("unsupported report operation or format")
        job_id = _uuid(value["job_id"], "job_id")
        report_token = _uuid(value["report_token"], "report_token")
        expected_sha256 = value["expected_sha256"]
        if not isinstance(expected_sha256, str) or len(expected_sha256) != 64:
            raise ProtocolError("expected_sha256 must be 64 lowercase hexadecimal characters")
        if any(character not in "0123456789abcdef" for character in expected_sha256):
            raise ProtocolError("expected_sha256 must be 64 lowercase hexadecimal characters")
        contract_path = _contained_existing_file(
            value["contract_path"], allowed_root, "contract_path"
        )
        output_value = value["output_path"]
        if not isinstance(output_value, str) or not output_value:
            raise ProtocolError("output_path must be a non-empty string")
        output_path = Path(output_value)
        if not output_path.is_absolute() or output_path.suffix.lower() != ".pdf":
            raise ProtocolError("output_path must be an absolute PDF path")
        output_parent = output_path.parent.resolve(strict=True)
        root = work_root.resolve(strict=True)
        try:
            output_parent.relative_to(root)
        except ValueError as error:
            raise ProtocolError("output_path is outside the allowed work root") from error
        if output_path.exists():
            raise ProtocolError("output_path must not already exist")
        return cls(
            protocol_version=PROTOCOL_VERSION,
            job_id=job_id,
            operation="render_report",
            report_token=report_token,
            contract_path=contract_path,
            expected_sha256=expected_sha256,
            output_path=output_path,
            format="pdf",
        )


def _uuid(value: Any, field: str) -> str:
    if not isinstance(value, str):
        raise ProtocolError(f"{field} must be a UUID string")
    try:
        parsed = UUID(value)
    except ValueError as error:
        raise ProtocolError(f"{field} must be a UUID string") from error
    if str(parsed) != value.lower():
        raise ProtocolError(f"{field} must use canonical UUID form")
    return str(parsed)


def _contained_existing_file(value: Any, root: Path, field: str) -> Path:
    if not isinstance(value, str) or not value:
        raise ProtocolError(f"{field} must be a non-empty string")
    path = Path(value)
    if not path.is_absolute():
        raise ProtocolError(f"{field} must be absolute")
    resolved_root = root.resolve(strict=True)
    resolved_path = path.resolve(strict=True)
    try:
        resolved_path.relative_to(resolved_root)
    except ValueError as error:
        raise ProtocolError(f"{field} is outside the allowed root") from error
    if not resolved_path.is_file():
        raise ProtocolError(f"{field} is not a regular file")
    return resolved_path
