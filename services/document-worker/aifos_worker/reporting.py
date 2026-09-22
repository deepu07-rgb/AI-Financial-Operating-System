"""Restricted PDF renderer for frozen, validated offline report contracts."""

from __future__ import annotations

import html
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from reportlab.graphics.shapes import Drawing, Rect, String
from reportlab.lib import colors
from reportlab.lib.enums import TA_CENTER
from reportlab.lib.pagesizes import A4, landscape
from reportlab.lib.styles import ParagraphStyle, getSampleStyleSheet
from reportlab.lib.units import mm
from reportlab.pdfgen import canvas
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.platypus import (
    KeepTogether,
    PageBreak,
    Paragraph,
    SimpleDocTemplate,
    Spacer,
    Table,
    TableStyle,
)

MAX_TABLE_ROWS = 10_000
REPORT_FONT = "Helvetica"
_unicode_font_path = Path("C:/Windows/Fonts/ARIALUNI.TTF")
if _unicode_font_path.is_file():
    try:
        pdfmetrics.registerFont(TTFont("AIFOSUnicode", str(_unicode_font_path)))
        REPORT_FONT = "AIFOSUnicode"
    except Exception:
        REPORT_FONT = "Helvetica"
EXPECTED_KEYS = {
    "report_id",
    "workspace_id",
    "template_id",
    "template_version",
    "template_key",
    "title",
    "period_from",
    "period_to",
    "currency",
    "filters",
    "metrics",
    "tables",
    "charts",
    "data_quality",
    "reconciliation",
    "source_references",
    "executive_summary",
    "recommended_actions",
    "further_questions",
    "caveats",
    "boundary_statement",
    "source_coverage_statement",
    "notes",
    "branding",
    "generated_at",
    "generator",
}


class ReportContractError(ValueError):
    """Raised when a frozen contract is unsafe or malformed."""


def validate_contract(value: Any) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != EXPECTED_KEYS:
        raise ReportContractError("report contract has missing or unknown fields")
    for key in ("report_id", "workspace_id", "template_id", "template_key", "title", "currency", "notes", "generator"):
        if not isinstance(value[key], str):
            raise ReportContractError(f"{key} must be text")
    if not isinstance(value["template_version"], int) or value["template_version"] < 1:
        raise ReportContractError("template_version must be positive")
    if not isinstance(value["metrics"], list) or not isinstance(value["tables"], list) or not isinstance(value["charts"], list):
        raise ReportContractError("metrics, tables, and charts must be arrays")
    row_count = 0
    for table in value["tables"]:
        if not isinstance(table, dict) or not isinstance(table.get("rows"), list):
            raise ReportContractError("table rows are invalid")
        row_count += len(table["rows"])
    if row_count > MAX_TABLE_ROWS:
        raise ReportContractError("report exceeds the PDF row limit")
    branding = value.get("branding")
    if not isinstance(branding, dict) or not all(
        isinstance(branding.get(key), str)
        for key in ("company_name", "report_title", "accent_hex", "footer_text")
    ):
        raise ReportContractError("branding is invalid")
    accent = branding["accent_hex"]
    if len(accent) != 7 or not accent.startswith("#"):
        raise ReportContractError("branding accent is invalid")
    reconciliation = value.get("reconciliation")
    if not isinstance(reconciliation, dict) or not all(
        isinstance(reconciliation.get(key), int)
        for key in ("source_record_count", "included_record_count", "excluded_record_count")
    ):
        raise ReportContractError("report reconciliation is invalid")
    for key in ("boundary_statement", "source_coverage_statement"):
        if not isinstance(value.get(key), str) or not value[key].strip():
            raise ReportContractError(f"{key} is required")
    return value


def render_report_pdf(contract: dict[str, Any], output_path: Path) -> int:
    contract = validate_contract(contract)
    accent = colors.HexColor(contract["branding"]["accent_hex"])
    management_pack = contract["template_key"] == "RPT-010"
    pagesize = A4 if management_pack else landscape(A4)
    document = SimpleDocTemplate(
        str(output_path),
        pagesize=pagesize,
        leftMargin=16 * mm,
        rightMargin=16 * mm,
        topMargin=20 * mm,
        bottomMargin=18 * mm,
        title=ascii_text(contract["title"]),
        author="AIFOS offline-report-engine",
        pageCompression=1,
    )
    styles = getSampleStyleSheet()
    for style_name in ("Normal", "BodyText", "Heading1", "Heading2", "Heading3", "Title"):
        styles[style_name].fontName = REPORT_FONT
    styles.add(ParagraphStyle(name="ReportTitle", parent=styles["Title"], textColor=accent, fontSize=22, leading=26, spaceAfter=10))
    styles.add(ParagraphStyle(name="PageTitle", parent=styles["Heading1"], textColor=accent, fontSize=16, leading=20, spaceAfter=8))
    styles.add(ParagraphStyle(name="SmallMuted", parent=styles["BodyText"], textColor=colors.HexColor("#56645F"), fontSize=8, leading=10))
    story: list[Any] = []
    if management_pack:
        _management_pages(story, contract, styles, accent)
    else:
        _standard_report(story, contract, styles, accent)

    def numbered_canvas(filename: str, **kwargs: Any) -> canvas.Canvas:
        kwargs["invariant"] = 1
        return canvas.Canvas(filename, **kwargs)

    document.build(
        story,
        onFirstPage=lambda pdf, doc: _page_frame(pdf, doc, contract, accent),
        onLaterPages=lambda pdf, doc: _page_frame(pdf, doc, contract, accent),
        canvasmaker=numbered_canvas,
    )
    return _pdf_page_count(output_path)


def _standard_report(story: list[Any], contract: dict[str, Any], styles: Any, accent: colors.Color) -> None:
    story.extend(_cover(contract, styles))
    story.extend(_reconciliation_panel(contract, styles, accent))
    for table in contract["tables"]:
        story.extend(_table_section(table, styles, accent, 80, wide=True))
    for chart in contract["charts"]:
        story.extend([Paragraph(safe(chart.get("title", "Chart")), styles["PageTitle"]), _bar_chart(chart, accent), Spacer(1, 6 * mm)])
    if contract["notes"].strip():
        story.extend([Paragraph("User-entered notes", styles["PageTitle"]), Paragraph(safe(contract["notes"]), styles["BodyText"])])


def _management_pages(story: list[Any], contract: dict[str, Any], styles: Any, accent: colors.Color) -> None:
    tables = {table.get("id"): table for table in contract["tables"]}
    charts = {chart.get("id"): chart for chart in contract["charts"]}
    story.extend(_cover(contract, styles))
    story.append(Paragraph("Executive summary", styles["PageTitle"]))
    story.extend(_bullet_list(contract.get("executive_summary", []), styles))
    story.extend(_metric_grid(contract["metrics"], styles, accent))
    story.extend(_reconciliation_panel(contract, styles, accent))
    story.append(Paragraph("Coverage cautions", styles["PageTitle"]))
    for warning in contract["data_quality"].get("warnings", []):
        story.append(Paragraph("Warning: " + safe(warning), styles["BodyText"]))
    story.append(PageBreak())

    story.append(Paragraph("2. Cash and obligations", styles["PageTitle"]))
    story.append(Paragraph("Cash and payment-clearance sources are shown as unavailable unless trusted data exists.", styles["BodyText"]))
    story.extend(_table_section(tables.get("upcoming_obligations", _empty_table("upcoming_obligations", "Upcoming obligations")), styles, accent, 18))
    story.append(PageBreak())

    story.append(Paragraph("3. Supplier and customer movements", styles["PageTitle"]))
    story.append(_bar_chart(charts.get("counterparty_spend", {"points": [], "title": "Counterparty spend"}), accent))
    story.extend(_table_section(tables.get("counterparties", _empty_table("counterparties", "Counterparties")), styles, accent, 14))
    story.append(PageBreak())

    story.append(Paragraph("4. Exceptions and controls", styles["PageTitle"]))
    story.extend(_table_section(tables.get("exceptions", _empty_table("exceptions", "Exceptions")), styles, accent, 20))
    story.append(PageBreak())

    story.append(Paragraph("5. Decisions, actions, and caveats", styles["PageTitle"]))
    story.append(Paragraph("User-entered notes", styles["Heading2"]))
    story.append(Paragraph(safe(contract["notes"] or "No notes entered."), styles["BodyText"]))
    story.append(Spacer(1, 8 * mm))
    story.append(Paragraph("Recommended next actions", styles["Heading2"]))
    story.extend(_bullet_list(contract.get("recommended_actions", []), styles, checkbox=True))
    story.append(Spacer(1, 5 * mm))
    story.append(Paragraph("Further questions", styles["Heading2"]))
    story.extend(_bullet_list(contract.get("further_questions", []), styles))
    story.append(Spacer(1, 5 * mm))
    story.append(Paragraph("Caveats and assumptions", styles["Heading2"]))
    story.extend(_bullet_list(contract.get("caveats", []), styles))


def _cover(contract: dict[str, Any], styles: Any) -> list[Any]:
    return [
        Paragraph(safe(contract["branding"]["company_name"]), styles["Heading2"]),
        Paragraph(safe(contract["branding"]["report_title"]), styles["ReportTitle"]),
        Paragraph(f"Source period: {safe(contract['period_from'])} to {safe(contract['period_to'])}", styles["BodyText"]),
        Paragraph(f"Generated: {safe(generated_time(contract['generated_at']))} | Currency: {safe(contract['currency'])}", styles["SmallMuted"]),
        Paragraph("Boundary: " + safe(contract["boundary_statement"]), styles["SmallMuted"]),
        Paragraph(safe(contract["source_coverage_statement"]), styles["SmallMuted"]),
        Spacer(1, 8 * mm),
    ]


def _reconciliation_panel(contract: dict[str, Any], styles: Any, accent: colors.Color) -> list[Any]:
    reconciliation = contract["reconciliation"]
    values = [
        ["Source records", str(reconciliation["source_record_count"])],
        ["Included", str(reconciliation["included_record_count"])],
        ["Excluded", str(reconciliation["excluded_record_count"])],
        ["Currencies", ", ".join(reconciliation.get("currencies", [])) or "None"],
        ["Generated by", f"{reconciliation.get('generated_by_name', '')} ({reconciliation.get('generated_by_role', '')})"],
        ["Period boundary", reconciliation.get("period_boundary", "")],
    ]
    table = Table(
        [[Paragraph(safe(label), styles["SmallMuted"]), Paragraph(safe(value), styles["SmallMuted"])] for label, value in values],
        colWidths=[38 * mm, 130 * mm],
    )
    table.setStyle(TableStyle([
        ("BOX", (0, 0), (-1, -1), 0.6, accent),
        ("INNERGRID", (0, 0), (-1, -1), 0.2, colors.HexColor("#D8E0DC")),
        ("BACKGROUND", (0, 0), (0, -1), colors.HexColor("#EEF4F0")),
        ("VALIGN", (0, 0), (-1, -1), "TOP"),
        ("LEFTPADDING", (0, 0), (-1, -1), 6),
        ("BOTTOMPADDING", (0, 0), (-1, -1), 5),
    ]))
    output: list[Any] = [Paragraph("Reconciliation before export", styles["PageTitle"]), table]
    for heading, items in (
        ("Exclusions", reconciliation.get("exclusions", [])),
        ("Missing sources", reconciliation.get("missing_sources", [])),
    ):
        if items:
            output.extend([Paragraph(heading, styles["Heading3"]), *_bullet_list(items, styles)])
    output.append(Spacer(1, 7 * mm))
    return output


def _bullet_list(items: list[Any], styles: Any, checkbox: bool = False) -> list[Any]:
    prefix = "[ ] " if checkbox else "- "
    return [Paragraph(prefix + safe(str(item)), styles["BodyText"]) for item in items]


def _metric_grid(metrics: list[dict[str, Any]], styles: Any, accent: colors.Color) -> list[Any]:
    cells = []
    for metric in metrics[:8]:
        value = metric.get("value_minor_units")
        if value is not None:
            rendered = f"{value / 100:,.2f} {metric.get('currency') or ''}".strip()
        elif metric.get("value_count") is not None:
            rendered = str(metric["value_count"])
        else:
            rendered = "Unavailable"
        cells.append([Paragraph(safe(metric.get("label", "Metric")), styles["SmallMuted"]), Paragraph(safe(rendered), styles["Heading2"])])
    if not cells:
        return []
    pairs = [cells[index:index + 2] for index in range(0, len(cells), 2)]
    grid = Table(pairs, colWidths=[82 * mm, 82 * mm])
    grid.setStyle(TableStyle([("BOX", (0, 0), (-1, -1), 0.5, accent), ("VALIGN", (0, 0), (-1, -1), "TOP"), ("LEFTPADDING", (0, 0), (-1, -1), 8), ("BOTTOMPADDING", (0, 0), (-1, -1), 8)]))
    return [grid, Spacer(1, 8 * mm)]


def _table_section(
    table: dict[str, Any],
    styles: Any,
    accent: colors.Color,
    limit: int,
    wide: bool = False,
) -> list[Any]:
    title = table.get("title", "Table")
    columns = table.get("columns", [])
    rows = table.get("rows", [])[:limit]
    elements: list[Any] = [Paragraph(safe(title), styles["PageTitle"])]
    note = table.get("availability_note")
    if note:
        elements.append(Paragraph("Coverage note: " + safe(note), styles["SmallMuted"]))
    if not columns:
        elements.append(Paragraph("No columns are available for this source.", styles["BodyText"]))
        return elements
    data = [[ascii_text(column.get("label", column.get("key", "")).replace(" (minor units)", "")) for column in columns]]
    for row in rows:
        data.append(
            [
                Paragraph(
                    safe(display_cell(row.get(column.get("key")), column.get("key", ""))),
                    styles["SmallMuted"],
                )
                for column in columns
            ]
        )
    if len(data) == 1:
        data.append(["No records in this frozen period"] + [""] * (len(columns) - 1))
    available_width = (260 if wide else 175) * mm
    rendered = Table(data, repeatRows=1, colWidths=[available_width / len(columns)] * len(columns))
    rendered.setStyle(TableStyle([("BACKGROUND", (0, 0), (-1, 0), accent), ("TEXTCOLOR", (0, 0), (-1, 0), colors.white), ("FONTNAME", (0, 0), (-1, 0), "Helvetica-Bold"), ("FONTSIZE", (0, 0), (-1, -1), 7), ("GRID", (0, 0), (-1, -1), 0.25, colors.HexColor("#C8D1CD")), ("VALIGN", (0, 0), (-1, -1), "TOP"), ("ROWBACKGROUNDS", (0, 1), (-1, -1), [colors.white, colors.HexColor("#F4F7F5")])]))
    elements.extend([rendered, Spacer(1, 6 * mm)])
    return elements


def _bar_chart(chart: dict[str, Any], accent: colors.Color) -> Drawing:
    drawing = Drawing(470, 180)
    points = chart.get("points", [])[:10]
    if not points:
        drawing.add(String(10, 80, "Chart unavailable for this frozen source coverage.", fontName=REPORT_FONT, fontSize=9))
        return drawing
    maximum = max(abs(int(point.get("value", 0))) for point in points) or 1
    row_height = min(24, 150 / len(points))
    for index, point in enumerate(points):
        value = int(point.get("value", 0))
        width = 260 * abs(value) / maximum
        y = 160 - index * row_height
        label = ascii_text(str(point.get("label", "")))[:34]
        drawing.add(String(5, y + 3, label, fontName=REPORT_FONT, fontSize=7))
        drawing.add(Rect(155, y, width, max(7, row_height - 7), fillColor=accent, strokeColor=None))
        drawing.add(String(160 + width, y + 3, exact_decimal(value), fontName=REPORT_FONT, fontSize=7))
    return drawing


def _page_frame(pdf: canvas.Canvas, document: SimpleDocTemplate, contract: dict[str, Any], accent: colors.Color) -> None:
    pdf.saveState()
    width, height = document.pagesize
    pdf.setStrokeColor(accent)
    pdf.line(16 * mm, height - 13 * mm, width - 16 * mm, height - 13 * mm)
    pdf.setFont(REPORT_FONT, 7)
    pdf.setFillColor(colors.HexColor("#56645F"))
    pdf.drawString(16 * mm, 9 * mm, ascii_text(contract["branding"]["footer_text"])[:90])
    pdf.drawRightString(width - 16 * mm, 9 * mm, f"Page {document.page} | Frozen run {contract['report_id']}")
    pdf.restoreState()


def _empty_table(identifier: str, title: str) -> dict[str, Any]:
    return {"id": identifier, "title": title, "columns": [], "rows": [], "availability_note": "No trusted source data is available."}


def cell_text(value: Any) -> str:
    if value is None:
        return ""
    return str(value)


def display_cell(value: Any, key: str) -> str:
    if isinstance(value, int) and not isinstance(value, bool) and key.endswith("_minor_units"):
        return exact_decimal(value)
    return cell_text(value)


def exact_decimal(value: int) -> str:
    sign = "-" if value < 0 else ""
    absolute = abs(value)
    return f"{sign}{absolute // 100:,}.{absolute % 100:02d}"


def ascii_text(value: str) -> str:
    normalized = value.replace("\u2013", "-").replace("\u2014", "-")
    if REPORT_FONT == "AIFOSUnicode":
        return normalized
    return normalized.encode("ascii", "replace").decode("ascii")


def safe(value: str) -> str:
    return html.escape(ascii_text(value)).replace("\n", "<br/>")


def generated_time(value: Any) -> str:
    if isinstance(value, int) and not isinstance(value, bool):
        return datetime.fromtimestamp(value / 1000, timezone.utc).isoformat().replace(
            "+00:00", "Z"
        )
    return str(value)


def _pdf_page_count(path: Path) -> int:
    # ReportLab writes one /Type /Page marker per page plus one /Pages marker.
    data = path.read_bytes()
    return data.count(b"/Type /Page") - data.count(b"/Type /Pages")
