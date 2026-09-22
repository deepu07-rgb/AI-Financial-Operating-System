//! Deterministic, persistence-independent financial domain primitives.
//!
//! Monetary values never use binary floating point. Dates use a strict
//! proleptic-Gregorian `YYYY-MM-DD` representation, and timestamps are signed
//! Unix milliseconds supplied by an application clock.

mod intelligence;
mod operations;
mod reports;
mod rules;

use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;
use uuid::Uuid;

pub use intelligence::{
    rank_payment_priorities, PaymentPriorityDecision, PaymentPriorityInput, PriorityComponent,
    PAYMENT_PRIORITY_RULE_VERSION,
};
pub use operations::{
    reconcile_obligation, CashBucketKind, CashScenarioBucket, CashSnapshot, InvoiceDirection,
    ObligationBalance, ObligationEvent, ObligationEventKind, ObligationState,
};
pub use reports::{
    build_invoice_table, FrozenReportContract, ReportBranding, ReportCell, ReportChart,
    ReportChartPoint, ReportColumn, ReportDataQuality, ReportDataset, ReportFilters,
    ReportInvoiceRow, ReportMetric, ReportPeriodBasis, ReportReconciliation, ReportSort,
    ReportSortDirection, ReportSourceReference, ReportTable, ReportTableColumn,
    ReportTemplateDefinition, REPORT_ENGINE_VERSION, REPORT_QUERY_ID_EXCEPTIONS,
    REPORT_QUERY_ID_INVOICES,
};
pub use rules::{
    evaluate_invoice_rules, RuleEvaluationConfig, RuleFinding, RuleInputSnapshot,
    RuleInvoiceSnapshot, RuleLineSnapshot, RuleSeverity, ThresholdRule, RULE_ENGINE_VERSION,
};

const MAX_DECIMAL_SCALE: u8 = 9;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DomainError {
    #[error("currency code must be exactly three uppercase ASCII letters")]
    InvalidCurrencyCode,
    #[error("cannot combine money values with different currencies")]
    CurrencyMismatch,
    #[error("money arithmetic overflowed its 64-bit minor-unit range")]
    MoneyOverflow,
    #[error("decimal scale must be at most {MAX_DECIMAL_SCALE}")]
    InvalidDecimalScale,
    #[error("date must be a valid ISO calendar date in YYYY-MM-DD form")]
    InvalidDate,
    #[error("document media type is not supported")]
    InvalidDocumentMediaType,
    #[error("identifier must be a valid UUID")]
    InvalidEntityId,
    #[error("text field {field} must not be blank")]
    BlankField { field: &'static str },
    #[error("invalid report definition: {0}")]
    InvalidReportDefinition(&'static str),
    #[error("obligation event is invalid or does not belong to this ledger")]
    InvalidObligationEvent,
    #[error("settlements and write-offs exceed the invoice total")]
    ObligationOverApplied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EntityId(Uuid);

impl EntityId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    #[must_use]
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for EntityId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for EntityId {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(value)
            .map(Self)
            .map_err(|_| DomainError::InvalidEntityId)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CurrencyCode([u8; 3]);

impl CurrencyCode {
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        let bytes = value.as_bytes();
        if bytes.len() != 3 || !bytes.iter().all(u8::is_ascii_uppercase) {
            return Err(DomainError::InvalidCurrencyCode);
        }
        Ok(Self([bytes[0], bytes[1], bytes[2]]))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).expect("currency code is validated ASCII")
    }
}

impl fmt::Display for CurrencyCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for CurrencyCode {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl Serialize for CurrencyCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for CurrencyCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    pub minor_units: i64,
    pub currency: CurrencyCode,
}

impl Money {
    #[must_use]
    pub const fn new(minor_units: i64, currency: CurrencyCode) -> Self {
        Self {
            minor_units,
            currency,
        }
    }

    pub fn checked_add(self, other: Self) -> Result<Self, DomainError> {
        self.ensure_same_currency(other)?;
        self.minor_units
            .checked_add(other.minor_units)
            .map(|minor_units| Self::new(minor_units, self.currency))
            .ok_or(DomainError::MoneyOverflow)
    }

    pub fn checked_sub(self, other: Self) -> Result<Self, DomainError> {
        self.ensure_same_currency(other)?;
        self.minor_units
            .checked_sub(other.minor_units)
            .map(|minor_units| Self::new(minor_units, self.currency))
            .ok_or(DomainError::MoneyOverflow)
    }

    fn ensure_same_currency(self, other: Self) -> Result<(), DomainError> {
        if self.currency == other.currency {
            Ok(())
        } else {
            Err(DomainError::CurrencyMismatch)
        }
    }
}

/// A fixed-precision decimal for quantities and rates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixedDecimal {
    pub coefficient: i64,
    pub scale: u8,
}

impl FixedDecimal {
    pub fn new(coefficient: i64, scale: u8) -> Result<Self, DomainError> {
        if scale > MAX_DECIMAL_SCALE {
            return Err(DomainError::InvalidDecimalScale);
        }
        Ok(Self { coefficient, scale })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LocalDate(i32);

impl LocalDate {
    pub fn from_ymd(year: i32, month: u8, day: u8) -> Result<Self, DomainError> {
        if !(1..=9999).contains(&year) || !(1..=12).contains(&month) {
            return Err(DomainError::InvalidDate);
        }
        let maximum_day = days_in_month(year, month);
        if day == 0 || day > maximum_day {
            return Err(DomainError::InvalidDate);
        }
        Ok(Self(
            year * 10_000 + i32::from(month) * 100 + i32::from(day),
        ))
    }

    #[must_use]
    pub const fn year(self) -> i32 {
        self.0 / 10_000
    }

    #[must_use]
    pub const fn month(self) -> u8 {
        ((self.0 / 100) % 100) as u8
    }

    #[must_use]
    pub const fn day(self) -> u8 {
        (self.0 % 100) as u8
    }

    #[must_use]
    pub fn days_since_unix_epoch(self) -> i64 {
        let month = self.month() as i32;
        let adjusted_year = self.year() - if month <= 2 { 1 } else { 0 };
        let era = adjusted_year / 400;
        let year_of_era = adjusted_year - era * 400;
        let adjusted_month = month + if month > 2 { -3 } else { 9 };
        let day_of_year = (153 * adjusted_month + 2) / 5 + i32::from(self.day()) - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        i64::from(era * 146_097 + day_of_era - 719_468)
    }

    #[must_use]
    pub fn days_until(self, other: Self) -> i64 {
        other.days_since_unix_epoch() - self.days_since_unix_epoch()
    }
}

impl fmt::Display for LocalDate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:04}-{:02}-{:02}",
            self.year(),
            self.month(),
            self.day()
        )
    }
}

impl FromStr for LocalDate {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let bytes = value.as_bytes();
        if bytes.len() != 10
            || bytes[4] != b'-'
            || bytes[7] != b'-'
            || bytes
                .iter()
                .enumerate()
                .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
        {
            return Err(DomainError::InvalidDate);
        }
        let year = value[0..4]
            .parse::<i32>()
            .map_err(|_| DomainError::InvalidDate)?;
        let month = value[5..7]
            .parse::<u8>()
            .map_err(|_| DomainError::InvalidDate)?;
        let day = value[8..10]
            .parse::<u8>()
            .map_err(|_| DomainError::InvalidDate)?;
        Self::from_ymd(year, month, day)
    }
}

impl Serialize for LocalDate {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for LocalDate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

const fn is_leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

const fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        2 if is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UtcTimestamp(i64);

impl UtcTimestamp {
    #[must_use]
    pub const fn from_unix_milliseconds(value: i64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn unix_milliseconds(self) -> i64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalRole {
    Owner,
    FinanceOperator,
    Reviewer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartyKind {
    Vendor,
    Customer,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceStatus {
    Draft,
    InReview,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewEventKind {
    Correction,
    Approval,
    Rejection,
    Comment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    Imported,
    Processing,
    NeedsReview,
    Approved,
    Rejected,
    Exported,
}

impl ReviewStatus {
    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Imported, Self::Processing)
                | (Self::Processing, Self::NeedsReview | Self::Rejected)
                | (
                    Self::NeedsReview,
                    Self::Processing | Self::Approved | Self::Rejected
                )
                | (Self::Rejected, Self::NeedsReview)
                | (Self::Approved, Self::Exported)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentMediaType {
    Pdf,
    Jpeg,
    Png,
    Unknown,
}

impl DocumentMediaType {
    #[must_use]
    pub const fn as_mime_type(self) -> &'static str {
        match self {
            Self::Pdf => "application/pdf",
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Unknown => "application/octet-stream",
        }
    }

    pub fn from_mime_type(value: &str) -> Result<Self, DomainError> {
        match value {
            "application/pdf" => Ok(Self::Pdf),
            "image/jpeg" => Ok(Self::Jpeg),
            "image/png" => Ok(Self::Png),
            "application/octet-stream" => Ok(Self::Unknown),
            _ => Err(DomainError::InvalidDocumentMediaType),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: EntityId,
    pub name: String,
    pub base_currency: CurrencyCode,
    pub locale: String,
    pub created_at: UtcTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspacePreferences {
    pub workspace_id: EntityId,
    pub financial_year_start_month: u8,
    pub date_format: String,
    pub tax_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalUser {
    pub id: EntityId,
    pub display_name: String,
    pub role: LocalRole,
    pub created_at: UtcTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Party {
    pub id: EntityId,
    pub kind: PartyKind,
    pub legal_name: String,
    pub tax_identifier: Option<String>,
    pub created_at: UtcTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    pub id: EntityId,
    pub sha256: String,
    pub original_filename: String,
    pub managed_path: String,
    pub byte_size: u64,
    pub media_type: DocumentMediaType,
    pub imported_at: UtcTimestamp,
    pub replaces_document_id: Option<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractionRun {
    pub id: EntityId,
    pub document_id: EntityId,
    pub protocol_version: u16,
    pub pipeline_version: String,
    pub engine_name: String,
    pub engine_version: String,
    pub language: String,
    pub result_json: String,
    pub requires_review: bool,
    pub started_at: UtcTimestamp,
    pub completed_at: UtcTimestamp,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentReviewTransition {
    pub id: EntityId,
    pub document_id: EntityId,
    pub extraction_run_id: Option<EntityId>,
    pub actor_user_id: Option<EntityId>,
    pub from_status: Option<ReviewStatus>,
    pub to_status: ReviewStatus,
    pub reason: String,
    pub occurred_at: UtcTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldCorrection {
    pub id: EntityId,
    pub document_id: EntityId,
    pub extraction_run_id: EntityId,
    pub actor_user_id: EntityId,
    pub field_name: String,
    pub original_value_json: String,
    pub prior_value_json: String,
    pub new_value_json: String,
    pub reason: String,
    pub occurred_at: UtcTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewComment {
    pub id: EntityId,
    pub document_id: EntityId,
    pub extraction_run_id: Option<EntityId>,
    pub actor_user_id: EntityId,
    pub body: String,
    pub tags: Vec<String>,
    pub occurred_at: UtcTimestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleImplementationStatus {
    Active,
    ExistingBoundary,
    AwaitingInput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleDefinition {
    pub id: EntityId,
    pub rule_id: String,
    pub version: u32,
    pub name: String,
    pub enabled: bool,
    pub severity: RuleSeverity,
    pub config_json: String,
    pub explanation_template: String,
    pub implementation_status: RuleImplementationStatus,
    pub created_at: UtcTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleEvent {
    pub id: EntityId,
    pub rule_id: String,
    pub rule_version: u32,
    pub subject_type: String,
    pub subject_id: EntityId,
    pub input_json: String,
    pub output_json: String,
    pub severity: RuleSeverity,
    pub occurred_at: UtcTimestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleEventActionKind {
    Acknowledge,
    Dismiss,
    Reopen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleEventStatus {
    Open,
    Acknowledged,
    Dismissed,
}

impl RuleEventStatus {
    #[must_use]
    pub const fn can_apply(self, action: RuleEventActionKind) -> bool {
        matches!(
            (self, action),
            (
                Self::Open,
                RuleEventActionKind::Acknowledge | RuleEventActionKind::Dismiss
            ) | (
                Self::Acknowledged | Self::Dismissed,
                RuleEventActionKind::Reopen
            )
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleEventAction {
    pub id: EntityId,
    pub rule_event_id: EntityId,
    pub actor_user_id: EntityId,
    pub action: RuleEventActionKind,
    pub reason: String,
    pub occurred_at: UtcTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashboardView {
    pub id: EntityId,
    pub version: u32,
    pub name: String,
    pub filters_json: String,
    pub widget_order_json: String,
    pub actor_user_id: Option<EntityId>,
    pub created_at: UtcTimestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportImplementationStatus {
    Active,
    PartialSourceCoverage,
    AwaitingInput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportTemplate {
    pub id: EntityId,
    pub template_key: String,
    pub version: u32,
    pub name: String,
    pub definition_json: String,
    pub implementation_status: ReportImplementationStatus,
    pub actor_user_id: Option<EntityId>,
    pub created_at: UtcTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportRun {
    pub id: EntityId,
    pub template_id: EntityId,
    pub template_version: u32,
    pub period_from: LocalDate,
    pub period_to: LocalDate,
    pub currency: CurrencyCode,
    pub contract_json: String,
    pub contract_sha256: String,
    pub row_count: u32,
    pub generator: String,
    pub actor_user_id: EntityId,
    pub generated_at: UtcTimestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportExportFormat {
    Csv,
    Pdf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportExport {
    pub id: EntityId,
    pub report_run_id: EntityId,
    pub format: ReportExportFormat,
    pub filename: String,
    pub sha256: String,
    pub byte_size: u64,
    pub actor_user_id: EntityId,
    pub exported_at: UtcTimestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportReminderCadence {
    Manual,
    Weekly,
    Monthly,
    Quarterly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportReminder {
    pub id: EntityId,
    pub version: u32,
    pub template_id: EntityId,
    pub cadence: ReportReminderCadence,
    pub next_due_date: Option<LocalDate>,
    pub enabled: bool,
    pub actor_user_id: EntityId,
    pub created_at: UtcTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceDraft {
    pub id: EntityId,
    pub document_id: EntityId,
    pub party_id: Option<EntityId>,
    pub invoice_number: Option<String>,
    pub invoice_date: Option<LocalDate>,
    pub due_date: Option<LocalDate>,
    pub total: Money,
    pub status: InvoiceStatus,
    pub version: u32,
    pub created_at: UtcTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceLine {
    pub id: EntityId,
    pub invoice_id: EntityId,
    pub invoice_version: u32,
    pub position: u32,
    pub description: String,
    pub quantity: FixedDecimal,
    pub unit_rate: Money,
    pub tax: Money,
    pub total: Money,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewEvent {
    pub id: EntityId,
    pub invoice_id: EntityId,
    pub invoice_version: u32,
    pub actor_user_id: EntityId,
    pub kind: ReviewEventKind,
    pub reason: String,
    pub field_name: Option<String>,
    pub prior_value_json: Option<String>,
    pub new_value_json: Option<String>,
    pub occurred_at: UtcTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: EntityId,
    pub actor_user_id: Option<EntityId>,
    pub action: String,
    pub entity_type: String,
    pub entity_id: EntityId,
    pub details_json: String,
    pub occurred_at: UtcTimestamp,
}

pub fn require_non_blank(value: &str, field: &'static str) -> Result<(), DomainError> {
    if value.trim().is_empty() {
        Err(DomainError::BlankField { field })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_arithmetic_is_checked_and_currency_safe() {
        let inr = CurrencyCode::parse("INR").unwrap();
        let usd = CurrencyCode::parse("USD").unwrap();
        assert_eq!(
            Money::new(12_345, inr)
                .checked_add(Money::new(655, inr))
                .unwrap(),
            Money::new(13_000, inr)
        );
        assert_eq!(
            Money::new(1, inr).checked_add(Money::new(1, usd)),
            Err(DomainError::CurrencyMismatch)
        );
        assert_eq!(
            Money::new(i64::MAX, inr).checked_add(Money::new(1, inr)),
            Err(DomainError::MoneyOverflow)
        );
    }

    #[test]
    fn currency_codes_are_canonical() {
        assert_eq!(CurrencyCode::parse("INR").unwrap().as_str(), "INR");
        for invalid in ["inr", "IN", "INR ", "12R", "₹₹₹"] {
            assert_eq!(
                CurrencyCode::parse(invalid),
                Err(DomainError::InvalidCurrencyCode)
            );
        }
    }

    #[test]
    fn dates_validate_iso_shape_and_gregorian_calendar() {
        assert_eq!(
            "2024-02-29".parse::<LocalDate>().unwrap().to_string(),
            "2024-02-29"
        );
        for invalid in ["2023-02-29", "2024-2-29", "0000-01-01", "2024-13-01"] {
            assert_eq!(invalid.parse::<LocalDate>(), Err(DomainError::InvalidDate));
        }
    }

    #[test]
    fn fixed_decimals_reject_unbounded_precision() {
        assert_eq!(
            FixedDecimal::new(1, 10),
            Err(DomainError::InvalidDecimalScale)
        );
        assert_eq!(
            FixedDecimal::new(12_500, 3).unwrap(),
            FixedDecimal {
                coefficient: 12_500,
                scale: 3
            }
        );
    }

    #[test]
    fn money_and_dates_use_canonical_json_strings() {
        let money = Money::new(12_345, CurrencyCode::parse("INR").unwrap());
        assert_eq!(
            serde_json::to_string(&money).unwrap(),
            r#"{"minor_units":12345,"currency":"INR"}"#
        );
        let date = "2026-08-04".parse::<LocalDate>().unwrap();
        assert_eq!(serde_json::to_string(&date).unwrap(), r#""2026-08-04""#);
        assert_eq!(
            serde_json::from_str::<LocalDate>(r#""2024-02-30""#)
                .unwrap_err()
                .to_string(),
            "date must be a valid ISO calendar date in YYYY-MM-DD form"
        );
    }

    #[test]
    fn review_status_transitions_are_explicit_and_terminal() {
        assert!(ReviewStatus::Imported.can_transition_to(ReviewStatus::Processing));
        assert!(ReviewStatus::Processing.can_transition_to(ReviewStatus::NeedsReview));
        assert!(ReviewStatus::NeedsReview.can_transition_to(ReviewStatus::Approved));
        assert!(ReviewStatus::Approved.can_transition_to(ReviewStatus::Exported));
        assert!(!ReviewStatus::Imported.can_transition_to(ReviewStatus::Approved));
        assert!(!ReviewStatus::Exported.can_transition_to(ReviewStatus::NeedsReview));
        assert!(!ReviewStatus::Approved.can_transition_to(ReviewStatus::Rejected));
    }
}
