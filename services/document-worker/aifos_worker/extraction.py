"""Conservative invoice-field extraction with source evidence."""

from __future__ import annotations

import re
from collections import defaultdict
from decimal import Decimal, InvalidOperation
from typing import Iterable

from .engines import OcrWord

FIELD_PATTERNS = {
    "invoice_number": re.compile(
        r"\b(?:invoice\b|inv\b)\s*(?:no\.?|number|#)?\s*[:#-]?\s*([A-Z0-9][A-Z0-9./_-]{2,})",
        re.IGNORECASE,
    ),
    "invoice_date": re.compile(
        r"\b(?:invoice\s+)?date\s*[:#-]?\s*(\d{1,4}[-/.]\d{1,2}[-/.]\d{1,4})",
        re.IGNORECASE,
    ),
    "due_date": re.compile(
        r"\bdue\s+date\s*[:#-]?\s*(\d{1,4}[-/.]\d{1,2}[-/.]\d{1,4})",
        re.IGNORECASE,
    ),
    "tax_identifier": re.compile(
        r"\b(?:GSTIN|GST\s*(?:No\.?|ID)|Tax\s*ID)\s*[:#-]?\s*([A-Z0-9-]{6,24})",
        re.IGNORECASE,
    ),
    "subtotal": re.compile(
        r"\b(?:sub\s*total|taxable\s+amount)\s*[:#-]?\s*(?:INR|USD|EUR|GBP|AED|Rs\.?|₹|\$|€|£)?\s*([0-9][0-9,]*(?:\.\d{1,2})?)",
        re.IGNORECASE,
    ),
    "tax": re.compile(
        r"\b(?:total\s+tax|tax|GST|VAT)\s*[:#-]?\s*(?:INR|USD|EUR|GBP|AED|Rs\.?|₹|\$|€|£)?\s*([0-9][0-9,]*(?:\.\d{1,2})?)",
        re.IGNORECASE,
    ),
    "total": re.compile(
        r"\b(?:grand\s+total|amount\s+due|total)\s*[:#-]?\s*(?:INR|USD|EUR|GBP|AED|Rs\.?|₹|\$|€|£)?\s*([0-9][0-9,]*(?:\.\d{1,2})?)",
        re.IGNORECASE,
    ),
    "payment_terms": re.compile(
        r"\b(?:payment\s+terms?|terms?)\s*[:#-]?\s*([A-Za-z0-9][A-Za-z0-9 /-]{1,40})",
        re.IGNORECASE,
    ),
}

CURRENCY_PATTERNS = [
    (re.compile(r"\bINR\b|₹|\bRs\.?") , "INR"),
    (re.compile(r"\bUSD\b|\$"), "USD"),
    (re.compile(r"\bEUR\b|€"), "EUR"),
    (re.compile(r"\bGBP\b|£"), "GBP"),
    (re.compile(r"\bAED\b"), "AED"),
]
BANK_PATTERN = re.compile(
    r"\b(?:bank|account|a/c|IBAN|SWIFT|IFSC)\b.*", re.IGNORECASE
)


def extract_fields(
    words: list[OcrWord], supplier_templates: list[dict[str, str]] | None = None
) -> tuple[dict[str, object], list[dict[str, object]], dict[str, object]]:
    lines = _group_lines(words)
    fields: dict[str, object] = {}
    for name, pattern in FIELD_PATTERNS.items():
        match = _first_match(lines, pattern)
        if match:
            text, evidence_words = match
            fields[name] = _field(text, evidence_words, untrusted=name == "payment_terms")

    supplier = _supplier_candidate(lines)
    if supplier:
        text, supplier_words = supplier
        fields["supplier"] = _field(text, supplier_words)

    applied_templates = _apply_supplier_templates(
        lines, fields, supplier_templates or []
    )

    for pattern, code in CURRENCY_PATTERNS:
        match = _first_match(lines, pattern, whole_match=True)
        if match:
            _, currency_words = match
            fields["currency"] = _field(code, currency_words)
            break

    bank_matches = []
    for line_text, line_words in lines:
        if BANK_PATTERN.search(line_text):
            bank_matches.append((line_text, line_words))
    if bank_matches:
        bank_text = " | ".join(text for text, _ in bank_matches)[:1000]
        bank_words = [word for _, items in bank_matches for word in items]
        fields["bank_details"] = _field(bank_text, bank_words, untrusted=True)

    validation = _validate_arithmetic(fields)
    warnings: list[dict[str, object]] = []
    if not words:
        warnings.append({"code": "no_text_detected", "requires_review": True})
    if "total" not in fields:
        warnings.append({"code": "total_not_found", "requires_review": True})
    if validation["status"] == "mismatch":
        warnings.append({"code": "arithmetic_mismatch", "requires_review": True})
    if "bank_details" in fields:
        warnings.append({"code": "bank_details_untrusted", "requires_review": True})
    warnings.append({"code": "line_items_not_reliably_extracted", "requires_review": True})
    for version_id in applied_templates:
        warnings.append({
            "code": "approved_supplier_template_applied",
            "template_version_id": version_id,
            "requires_review": True,
        })
    return fields, warnings, validation


def _apply_supplier_templates(
    lines: list[tuple[str, list[OcrWord]]],
    fields: dict[str, object],
    templates: list[dict[str, str]],
) -> list[str]:
    supplier = fields.get("supplier")
    if not isinstance(supplier, dict):
        return []
    supplier_key = str(supplier.get("value_text", "")).strip().casefold()
    field_map = {"supplier_name": "supplier", "total_amount": "total"}
    applied = []
    for template in templates:
        if template["supplier_key"].strip().casefold() != supplier_key:
            continue
        pattern = re.compile(
            rf"\b{re.escape(template['label_text'].strip())}\s*[:#-]?\s*(\S(?:.*\S)?)\s*$",
            re.IGNORECASE,
        )
        match = _first_match(lines, pattern)
        if not match:
            continue
        value, evidence_words = match
        if not value or len(value) > 160:
            continue
        field_name = field_map.get(template["field_name"], template["field_name"])
        fields[field_name] = _field(value, evidence_words, untrusted=True)
        applied.append(template["version_id"])
    return applied


def serialize_words(words: Iterable[OcrWord]) -> list[dict[str, object]]:
    return [
        {
            "text": word.text,
            "confidence": word.confidence,
            "bbox": {
                "left": word.left,
                "top": word.top,
                "width": word.width,
                "height": word.height,
            },
        }
        for word in words
    ]


def _group_lines(words: list[OcrWord]) -> list[tuple[str, list[OcrWord]]]:
    groups: dict[tuple[int, int, int, int], list[OcrWord]] = defaultdict(list)
    for word in words:
        groups[(word.page_index, word.block, word.paragraph, word.line)].append(word)
    lines = []
    for key in sorted(groups):
        line_words = sorted(groups[key], key=lambda word: (word.left, word.top))
        lines.append((" ".join(word.text for word in line_words), line_words))
    return lines


def _first_match(
    lines: list[tuple[str, list[OcrWord]]],
    pattern: re.Pattern[str],
    whole_match: bool = False,
) -> tuple[str, list[OcrWord]] | None:
    for text, words in lines:
        match = pattern.search(text)
        if match:
            return (match.group(0) if whole_match else match.group(1), words)
    return None


def _supplier_candidate(
    lines: list[tuple[str, list[OcrWord]]],
) -> tuple[str, list[OcrWord]] | None:
    excluded = re.compile(r"invoice|tax invoice|bill to|ship to", re.IGNORECASE)
    for text, words in lines[:6]:
        stripped = text.strip()
        if 3 <= len(stripped) <= 160 and any(character.isalpha() for character in stripped):
            if not excluded.fullmatch(stripped):
                return stripped, words
    return None


def _field(value_text: str, words: list[OcrWord], untrusted: bool = False) -> dict[str, object]:
    confidence = round(min((word.confidence for word in words), default=0.0), 4)
    return {
        "value_text": value_text.strip(),
        "confidence": confidence,
        "untrusted": untrusted,
        "sources": [
            {
                "page_index": word.page_index,
                "text": word.text,
                "bbox": {
                    "left": word.left,
                    "top": word.top,
                    "width": word.width,
                    "height": word.height,
                },
            }
            for word in words
        ],
    }


def _validate_arithmetic(fields: dict[str, object]) -> dict[str, object]:
    if not all(name in fields for name in ("subtotal", "tax", "total")):
        return {"status": "insufficient_fields", "tolerance_text": "0.01"}
    try:
        subtotal = _decimal(fields["subtotal"])
        tax = _decimal(fields["tax"])
        total = _decimal(fields["total"])
    except InvalidOperation:
        return {"status": "unparseable", "tolerance_text": "0.01"}
    delta = total - (subtotal + tax)
    return {
        "status": "valid" if abs(delta) <= Decimal("0.01") else "mismatch",
        "tolerance_text": "0.01",
        "delta_text": format(delta, "f"),
    }


def _decimal(field: object) -> Decimal:
    assert isinstance(field, dict)
    return Decimal(str(field["value_text"]).replace(",", ""))
