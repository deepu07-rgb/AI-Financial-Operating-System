//! Deterministic, persistence-independent report contracts and table shaping.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{DomainError, EntityId, LocalDate, UtcTimestamp};

pub const REPORT_ENGINE_VERSION: &str = "offline-report-engine/1";
pub const REPORT_QUERY_ID_INVOICES: &str = "Q-RPT-INVOICES-001";
pub const REPORT_QUERY_ID_EXCEPTIONS: &str = "Q-RPT-EXCEPTIONS-001";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportDataset {
    InvoiceRegister,
    PayablesDueSchedule,
    ReceivablesDueSchedule,
    SupplierPriceTrend,
    Exceptions,
    VendorSummary,
    AuditPack,
    ObligationsSettlements,
    CustomInvoices,
    ManagementPack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportColumn {
    InvoiceNumber,
    Direction,
    Party,
    IssueDate,
    DueDate,
    Total,
    Currency,
    Status,
    Confidence,
    Category,
    DocumentId,
}

impl ReportColumn {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::InvoiceNumber => "invoice_number",
            Self::Direction => "direction",
            Self::Party => "party",
            Self::IssueDate => "issue_date",
            Self::DueDate => "due_date",
            Self::Total => "total_minor_units",
            Self::Currency => "currency",
            Self::Status => "status",
            Self::Confidence => "confidence_basis_points",
            Self::Category => "category",
            Self::DocumentId => "document_id",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::InvoiceNumber => "Invoice number",
            Self::Direction => "Direction",
            Self::Party => "Party",
            Self::IssueDate => "Issue date",
            Self::DueDate => "Due date",
            Self::Total => "Amount (minor units)",
            Self::Currency => "Currency",
            Self::Status => "Status",
            Self::Confidence => "Confidence (basis points)",
            Self::Category => "Category",
            Self::DocumentId => "Source document ID",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportSortDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportPeriodBasis {
    IssueDate,
    DueDate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportFilters {
    pub period_from: LocalDate,
    pub period_to: LocalDate,
    pub party_id: Option<EntityId>,
    pub direction: Option<String>,
    pub status: Option<String>,
    pub confidence: Option<String>,
    pub category: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportSort {
    pub column: ReportColumn,
    pub direction: ReportSortDirection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportBranding {
    pub company_name: String,
    pub report_title: String,
    pub accent_hex: String,
    pub footer_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportTemplateDefinition {
    pub dataset: ReportDataset,
    pub period_basis: ReportPeriodBasis,
    pub columns: Vec<ReportColumn>,
    pub filters: ReportFilters,
    pub group_by: Option<ReportColumn>,
    pub sort: ReportSort,
    pub include_totals: bool,
    pub include_chart: bool,
    pub notes: String,
    pub branding: ReportBranding,
}

impl ReportTemplateDefinition {
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.filters.period_from > self.filters.period_to {
            return Err(DomainError::InvalidReportDefinition(
                "report period starts after it ends",
            ));
        }
        if self.columns.is_empty() || self.columns.len() > 11 {
            return Err(DomainError::InvalidReportDefinition(
                "report must contain between one and eleven columns",
            ));
        }
        let unique = self.columns.iter().copied().collect::<BTreeSet<_>>();
        if unique.len() != self.columns.len() {
            return Err(DomainError::InvalidReportDefinition(
                "report columns must be unique",
            ));
        }
        if let Some(group) = self.group_by {
            if !self.columns.contains(&group) {
                return Err(DomainError::InvalidReportDefinition(
                    "grouping column must also be selected",
                ));
            }
        }
        if !self.columns.contains(&self.sort.column) {
            return Err(DomainError::InvalidReportDefinition(
                "sort column must also be selected",
            ));
        }
        if self.branding.company_name.trim().is_empty()
            || self.branding.report_title.trim().is_empty()
        {
            return Err(DomainError::InvalidReportDefinition(
                "company and report titles must not be blank",
            ));
        }
        if !is_hex_color(&self.branding.accent_hex) {
            return Err(DomainError::InvalidReportDefinition(
                "accent color must use #RRGGBB form",
            ));
        }
        if self.notes.len() > 4_000 || self.branding.footer_text.len() > 300 {
            return Err(DomainError::InvalidReportDefinition(
                "notes or footer exceed their length limit",
            ));
        }
        Ok(())
    }
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..]
            .bytes()
            .all(|character| character.is_ascii_hexdigit())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportInvoiceRow {
    pub invoice_id: EntityId,
    pub document_id: EntityId,
    pub invoice_number: Option<String>,
    pub direction: String,
    pub party_id: Option<EntityId>,
    pub party_name: Option<String>,
    pub issue_date: Option<LocalDate>,
    pub due_date: Option<LocalDate>,
    pub total_minor_units: Option<i64>,
    pub currency: Option<String>,
    pub status: String,
    pub confidence_basis_points: Option<i64>,
    pub category: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ReportCell {
    Text(String),
    Integer(i64),
    Empty(Option<()>),
}

impl ReportCell {
    #[must_use]
    pub fn csv_value(&self) -> String {
        match self {
            Self::Text(value) => value.clone(),
            Self::Integer(value) => value.to_string(),
            Self::Empty(_) => String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportTable {
    pub id: String,
    pub title: String,
    pub columns: Vec<ReportTableColumn>,
    pub rows: Vec<BTreeMap<String, ReportCell>>,
    pub totals: BTreeMap<String, ReportCell>,
    pub source_query_id: String,
    pub availability_note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportTableColumn {
    pub key: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportMetric {
    pub key: String,
    pub label: String,
    pub value_minor_units: Option<i64>,
    pub value_count: Option<i64>,
    pub currency: Option<String>,
    pub source_query_id: String,
    pub availability_note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportChartPoint {
    pub label: String,
    pub value: i64,
    pub currency: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportChart {
    pub id: String,
    pub title: String,
    pub points: Vec<ReportChartPoint>,
    pub source_query_id: String,
    pub availability_note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportDataQuality {
    pub document_count: i64,
    pub approved_invoice_count: i64,
    pub review_pending: i64,
    pub payment_source_available: bool,
    pub cash_source_available: bool,
    pub category_source_available: bool,
    pub warnings: Vec<String>,
}

/// Frozen evidence pointer used by accountants to move from a report value back
/// to the approved operational record and its immutable source document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportSourceReference {
    pub record_id: EntityId,
    pub document_id: EntityId,
    pub source_filename: String,
    pub source_sha256: String,
    pub audit_event_count: u32,
}

/// Pre-export control totals. These values are part of the frozen contract, so
/// preview and every export disclose the same inclusion boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportReconciliation {
    pub source_record_count: u32,
    pub included_record_count: u32,
    pub excluded_record_count: u32,
    pub currencies: Vec<String>,
    pub exclusions: Vec<String>,
    pub missing_sources: Vec<String>,
    pub generated_by_user_id: EntityId,
    pub generated_by_name: String,
    pub generated_by_role: String,
    pub period_boundary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenReportContract {
    pub report_id: EntityId,
    pub workspace_id: EntityId,
    pub template_id: EntityId,
    pub template_version: i64,
    pub template_key: String,
    pub title: String,
    pub period_from: LocalDate,
    pub period_to: LocalDate,
    pub currency: String,
    pub filters: ReportFilters,
    pub metrics: Vec<ReportMetric>,
    pub tables: Vec<ReportTable>,
    pub charts: Vec<ReportChart>,
    pub data_quality: ReportDataQuality,
    pub reconciliation: ReportReconciliation,
    pub source_references: Vec<ReportSourceReference>,
    pub executive_summary: Vec<String>,
    pub recommended_actions: Vec<String>,
    pub further_questions: Vec<String>,
    pub caveats: Vec<String>,
    pub boundary_statement: String,
    pub source_coverage_statement: String,
    pub notes: String,
    pub branding: ReportBranding,
    pub generated_at: UtcTimestamp,
    pub generator: String,
}

pub fn build_invoice_table(
    id: &str,
    title: &str,
    definition: &ReportTemplateDefinition,
    rows: &[ReportInvoiceRow],
) -> Result<ReportTable, DomainError> {
    definition.validate()?;
    let mut selected = rows
        .iter()
        .filter(|row| invoice_matches(row, &definition.filters, definition.period_basis))
        .collect::<Vec<_>>();
    selected.sort_by(|left, right| {
        let group_ordering = definition
            .group_by
            .map(|group| cell_sort_value(left, group).cmp(&cell_sort_value(right, group)))
            .unwrap_or(std::cmp::Ordering::Equal);
        let ordering = group_ordering
            .then_with(|| {
                cell_sort_value(left, definition.sort.column)
                    .cmp(&cell_sort_value(right, definition.sort.column))
            })
            .then_with(|| {
                left.invoice_id
                    .to_string()
                    .cmp(&right.invoice_id.to_string())
            });
        match definition.sort.direction {
            ReportSortDirection::Ascending => ordering,
            ReportSortDirection::Descending => ordering.reverse(),
        }
    });
    let columns = definition
        .columns
        .iter()
        .map(|column| ReportTableColumn {
            key: column.key().to_owned(),
            label: column.label().to_owned(),
        })
        .collect::<Vec<_>>();
    let table_rows = selected
        .iter()
        .map(|row| {
            definition
                .columns
                .iter()
                .map(|column| (column.key().to_owned(), report_cell(row, *column)))
                .collect::<BTreeMap<_, _>>()
        })
        .collect::<Vec<_>>();
    let mut totals = BTreeMap::new();
    if definition.include_totals && definition.columns.contains(&ReportColumn::Total) {
        let currencies = selected
            .iter()
            .filter_map(|row| row.currency.as_deref())
            .collect::<BTreeSet<_>>();
        if currencies.len() == 1 {
            let total = selected.iter().try_fold(0_i64, |sum, row| {
                sum.checked_add(row.total_minor_units.unwrap_or_default())
                    .ok_or(DomainError::MoneyOverflow)
            })?;
            totals.insert(
                ReportColumn::Total.key().to_owned(),
                ReportCell::Integer(total),
            );
        }
    }
    Ok(ReportTable {
        id: id.to_owned(),
        title: title.to_owned(),
        columns,
        rows: table_rows,
        totals,
        source_query_id: REPORT_QUERY_ID_INVOICES.to_owned(),
        availability_note: None,
    })
}

fn invoice_matches(
    row: &ReportInvoiceRow,
    filters: &ReportFilters,
    period_basis: ReportPeriodBasis,
) -> bool {
    let period_date = match period_basis {
        ReportPeriodBasis::IssueDate => row.issue_date,
        ReportPeriodBasis::DueDate => row.due_date,
    };
    if period_date.is_none_or(|date| date < filters.period_from || date > filters.period_to) {
        return false;
    }
    if filters
        .party_id
        .is_some_and(|party_id| row.party_id != Some(party_id))
    {
        return false;
    }
    if filters
        .direction
        .as_deref()
        .is_some_and(|direction| row.direction != direction)
    {
        return false;
    }
    if filters
        .status
        .as_deref()
        .is_some_and(|status| row.status != status)
    {
        return false;
    }
    if filters
        .category
        .as_deref()
        .is_some_and(|category| row.category.as_deref() != Some(category))
    {
        return false;
    }
    match filters.confidence.as_deref() {
        Some("low") if row.confidence_basis_points.unwrap_or_default() >= 7_000 => false,
        Some("medium")
            if !(7_000..9_000).contains(&row.confidence_basis_points.unwrap_or_default()) =>
        {
            false
        }
        Some("high") if row.confidence_basis_points.unwrap_or_default() < 9_000 => false,
        _ => true,
    }
}

fn report_cell(row: &ReportInvoiceRow, column: ReportColumn) -> ReportCell {
    match column {
        ReportColumn::InvoiceNumber => optional_text(row.invoice_number.as_deref()),
        ReportColumn::Direction => ReportCell::Text(row.direction.clone()),
        ReportColumn::Party => optional_text(row.party_name.as_deref()),
        ReportColumn::IssueDate => {
            optional_text(row.issue_date.map(|date| date.to_string()).as_deref())
        }
        ReportColumn::DueDate => {
            optional_text(row.due_date.map(|date| date.to_string()).as_deref())
        }
        ReportColumn::Total => row
            .total_minor_units
            .map(ReportCell::Integer)
            .unwrap_or(ReportCell::Empty(None)),
        ReportColumn::Currency => optional_text(row.currency.as_deref()),
        ReportColumn::Status => ReportCell::Text(row.status.clone()),
        ReportColumn::Confidence => row
            .confidence_basis_points
            .map(ReportCell::Integer)
            .unwrap_or(ReportCell::Empty(None)),
        ReportColumn::Category => optional_text(row.category.as_deref()),
        ReportColumn::DocumentId => ReportCell::Text(row.document_id.to_string()),
    }
}

fn optional_text(value: Option<&str>) -> ReportCell {
    value
        .map(|value| ReportCell::Text(value.to_owned()))
        .unwrap_or(ReportCell::Empty(None))
}

fn cell_sort_value(row: &ReportInvoiceRow, column: ReportColumn) -> String {
    report_cell(row, column).csv_value()
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn definition() -> ReportTemplateDefinition {
        ReportTemplateDefinition {
            dataset: ReportDataset::CustomInvoices,
            period_basis: ReportPeriodBasis::IssueDate,
            columns: vec![
                ReportColumn::Party,
                ReportColumn::InvoiceNumber,
                ReportColumn::Total,
                ReportColumn::Currency,
            ],
            filters: ReportFilters {
                period_from: LocalDate::from_str("2026-01-01").unwrap(),
                period_to: LocalDate::from_str("2026-12-31").unwrap(),
                party_id: None,
                direction: Some("payable".to_owned()),
                status: None,
                confidence: None,
                category: None,
            },
            group_by: Some(ReportColumn::Party),
            sort: ReportSort {
                column: ReportColumn::Party,
                direction: ReportSortDirection::Ascending,
            },
            include_totals: true,
            include_chart: true,
            notes: String::new(),
            branding: ReportBranding {
                company_name: "Synthetic Works".to_owned(),
                report_title: "Supplier report".to_owned(),
                accent_hex: "#153F35".to_owned(),
                footer_text: "Confidential".to_owned(),
            },
        }
    }

    #[test]
    fn report_table_filters_sorts_and_totals_integer_money() {
        let party = EntityId::new();
        let rows = vec![
            ReportInvoiceRow {
                invoice_id: EntityId::new(),
                document_id: EntityId::new(),
                invoice_number: Some("B-2".to_owned()),
                direction: "payable".to_owned(),
                party_id: Some(party),
                party_name: Some("Beta".to_owned()),
                issue_date: Some(LocalDate::from_str("2026-02-01").unwrap()),
                due_date: None,
                total_minor_units: Some(2_500),
                currency: Some("INR".to_owned()),
                status: "approved".to_owned(),
                confidence_basis_points: Some(9_500),
                category: None,
            },
            ReportInvoiceRow {
                invoice_id: EntityId::new(),
                document_id: EntityId::new(),
                invoice_number: Some("A-1".to_owned()),
                direction: "payable".to_owned(),
                party_id: Some(party),
                party_name: Some("Alpha".to_owned()),
                issue_date: Some(LocalDate::from_str("2026-01-01").unwrap()),
                due_date: None,
                total_minor_units: Some(1_500),
                currency: Some("INR".to_owned()),
                status: "approved".to_owned(),
                confidence_basis_points: Some(9_000),
                category: None,
            },
        ];
        let table = build_invoice_table("invoices", "Invoices", &definition(), &rows).unwrap();
        assert_eq!(table.rows.len(), 2);
        assert_eq!(
            table.rows[0].get("party"),
            Some(&ReportCell::Text("Alpha".to_owned()))
        );
        assert_eq!(
            table.totals.get("total_minor_units"),
            Some(&ReportCell::Integer(4_000))
        );
    }

    #[test]
    fn definition_rejects_duplicate_columns_and_cross_currency_totals_are_omitted() {
        let mut invalid = definition();
        invalid.columns.push(ReportColumn::Party);
        assert!(invalid.validate().is_err());

        let mut valid = definition();
        valid.include_totals = true;
        let rows = [
            ReportInvoiceRow {
                invoice_id: EntityId::new(),
                document_id: EntityId::new(),
                invoice_number: None,
                direction: "payable".to_owned(),
                party_id: None,
                party_name: None,
                issue_date: None,
                due_date: None,
                total_minor_units: Some(100),
                currency: Some("INR".to_owned()),
                status: "approved".to_owned(),
                confidence_basis_points: None,
                category: None,
            },
            ReportInvoiceRow {
                currency: Some("USD".to_owned()),
                ..ReportInvoiceRow {
                    invoice_id: EntityId::new(),
                    document_id: EntityId::new(),
                    invoice_number: None,
                    direction: "payable".to_owned(),
                    party_id: None,
                    party_name: None,
                    issue_date: None,
                    due_date: None,
                    total_minor_units: Some(100),
                    currency: None,
                    status: "approved".to_owned(),
                    confidence_basis_points: None,
                    category: None,
                }
            },
        ];
        let table = build_invoice_table("invoices", "Invoices", &valid, &rows).unwrap();
        assert!(table.totals.is_empty());
    }

    #[test]
    fn ten_thousand_rows_and_empty_periods_are_deterministic() {
        let date = LocalDate::from_str("2026-06-15").unwrap();
        let rows = (0..10_000)
            .map(|index| ReportInvoiceRow {
                invoice_id: EntityId::new(),
                document_id: EntityId::new(),
                invoice_number: Some(format!("INV-{index:05}")),
                direction: "payable".to_owned(),
                party_id: None,
                party_name: Some(
                    "Bharat Precision Components and Industrial Automation Systems Private Limited"
                        .to_owned(),
                ),
                issue_date: Some(date),
                due_date: Some(date),
                total_minor_units: Some(101),
                currency: Some("INR".to_owned()),
                status: "approved".to_owned(),
                confidence_basis_points: None,
                category: None,
            })
            .collect::<Vec<_>>();
        let started = std::time::Instant::now();
        let table = build_invoice_table("large", "Large", &definition(), &rows).unwrap();
        assert_eq!(table.rows.len(), 10_000);
        assert_eq!(
            table.totals["total_minor_units"],
            ReportCell::Integer(1_010_000)
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(5));

        let mut empty = definition();
        empty.filters.period_from = LocalDate::from_str("2025-01-01").unwrap();
        empty.filters.period_to = LocalDate::from_str("2025-12-31").unwrap();
        assert!(build_invoice_table("empty", "Empty", &empty, &rows)
            .unwrap()
            .rows
            .is_empty());
    }
}
