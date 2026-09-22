//! Versioned, deterministic connector contracts and offline file adapters.

use std::collections::{BTreeMap, BTreeSet};

use aifos_domain::{CurrencyCode, LocalDate, PartyKind};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const CONNECTOR_SDK_VERSION: u16 = 1;
pub const MAX_SOURCE_BYTES: usize = 5 * 1024 * 1024;
pub const MAX_SOURCE_ROWS: usize = 10_000;
const MAX_COLUMNS: usize = 64;
const MAX_CELL_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ConnectorError {
    #[error("connector source exceeds the 5 MiB limit")]
    SourceTooLarge,
    #[error("connector source must be valid UTF-8 without a byte-order mark")]
    InvalidUtf8,
    #[error("connector source contains a forbidden XML construct")]
    UnsafeXml,
    #[error("connector source has invalid structure: {0}")]
    InvalidStructure(&'static str),
    #[error("connector mapping is invalid: {0}")]
    InvalidMapping(&'static str),
}

pub type Result<T> = std::result::Result<T, ConnectorError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorFormat {
    Csv,
    TallyXml,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorEntityKind {
    Party,
    Invoice,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorManifest {
    pub connector_key: &'static str,
    pub version: u16,
    pub display_name: &'static str,
    pub format: ConnectorFormat,
    pub supported_entities: &'static [ConnectorEntityKind],
    pub import_supported: bool,
    pub export_supported: bool,
    pub network_required: bool,
    pub credentials_required: bool,
    pub notes: &'static str,
}

pub const CSV_MANIFEST: ConnectorManifest = ConnectorManifest {
    connector_key: "csv",
    version: 1,
    display_name: "CSV / Excel-compatible",
    format: ConnectorFormat::Csv,
    supported_entities: &[ConnectorEntityKind::Party, ConnectorEntityKind::Invoice],
    import_supported: true,
    export_supported: true,
    network_required: false,
    credentials_required: false,
    notes: "Mapped UTF-8 CSV; exports are formula-neutralized.",
};

pub const TALLY_MANIFEST: ConnectorManifest = ConnectorManifest {
    connector_key: "tally_xml",
    version: 1,
    display_name: "TallyPrime XML file pilot",
    format: ConnectorFormat::TallyXml,
    supported_entities: &[ConnectorEntityKind::Party, ConnectorEntityKind::Invoice],
    import_supported: true,
    export_supported: false,
    network_required: false,
    credentials_required: false,
    notes: "Offline ledger/voucher XML staging; no HTTP server or automatic posting.",
};

#[must_use]
pub fn manifests() -> [&'static ConnectorManifest; 2] {
    [&CSV_MANIFEST, &TALLY_MANIFEST]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FieldMapping {
    pub entity_kind: ConnectorEntityKind,
    pub fields: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CanonicalParty {
    pub external_id: String,
    pub legal_name: String,
    pub kind: PartyKind,
    pub tax_identifier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CanonicalInvoice {
    pub external_id: String,
    pub party_external_id: Option<String>,
    pub party_name: String,
    pub invoice_number: String,
    pub invoice_date: LocalDate,
    pub due_date: Option<LocalDate>,
    pub total_minor_units: i64,
    pub currency: CurrencyCode,
    pub direction: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "entityKind", content = "record", rename_all = "snake_case")]
pub enum CanonicalRecord {
    Party(CanonicalParty),
    Invoice(CanonicalInvoice),
}

impl CanonicalRecord {
    #[must_use]
    pub fn external_id(&self) -> &str {
        match self {
            Self::Party(record) => &record.external_id,
            Self::Invoice(record) => &record.external_id,
        }
    }

    #[must_use]
    pub const fn entity_kind(&self) -> ConnectorEntityKind {
        match self {
            Self::Party(_) => ConnectorEntityKind::Party,
            Self::Invoice(_) => ConnectorEntityKind::Invoice,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RowError {
    pub field: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ParsedRow {
    pub row_number: u32,
    pub record: Option<CanonicalRecord>,
    pub errors: Vec<RowError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ParsedSource {
    pub sdk_version: u16,
    pub connector_key: String,
    pub connector_version: u16,
    pub source_sha256: String,
    pub entity_kind: ConnectorEntityKind,
    pub mapping: FieldMapping,
    pub rows: Vec<ParsedRow>,
}

impl ParsedSource {
    #[must_use]
    pub fn valid_count(&self) -> usize {
        self.rows.iter().filter(|row| row.record.is_some()).count()
    }

    #[must_use]
    pub fn invalid_count(&self) -> usize {
        self.rows.len().saturating_sub(self.valid_count())
    }
}

pub fn parse_csv(bytes: &[u8], mapping: FieldMapping) -> Result<ParsedSource> {
    let source = source_text(bytes)?;
    let rows = parse_csv_rows(source)?;
    if rows.len() < 2 {
        return Err(ConnectorError::InvalidStructure(
            "CSV requires a header and at least one data row",
        ));
    }
    let headers = &rows[0];
    validate_mapping(&mapping, headers)?;
    let header_indexes = headers
        .iter()
        .enumerate()
        .map(|(index, value)| (value.trim().to_owned(), index))
        .collect::<BTreeMap<_, _>>();
    let parsed_rows = rows
        .into_iter()
        .skip(1)
        .enumerate()
        .map(|(index, row)| {
            let row_number = u32::try_from(index + 2).unwrap_or(u32::MAX);
            let value = |field: &str| -> Option<&str> {
                mapping
                    .fields
                    .get(field)
                    .and_then(|column| header_indexes.get(column))
                    .and_then(|column| row.get(*column))
                    .map(|cell| cell.trim())
            };
            parse_mapped_row(mapping.entity_kind, row_number, value)
        })
        .collect();
    Ok(ParsedSource {
        sdk_version: CONNECTOR_SDK_VERSION,
        connector_key: CSV_MANIFEST.connector_key.to_owned(),
        connector_version: CSV_MANIFEST.version,
        source_sha256: sha256(bytes),
        entity_kind: mapping.entity_kind,
        mapping,
        rows: parsed_rows,
    })
}

pub fn inspect_csv_headers(bytes: &[u8]) -> Result<Vec<String>> {
    let source = source_text(bytes)?;
    let rows = parse_csv_rows(source)?;
    let headers = rows
        .first()
        .ok_or(ConnectorError::InvalidStructure("CSV header is missing"))?;
    if headers.is_empty() || headers.iter().any(|header| header.trim().is_empty()) {
        return Err(ConnectorError::InvalidStructure(
            "CSV headers must be non-empty",
        ));
    }
    let unique = headers
        .iter()
        .map(|header| header.trim())
        .collect::<BTreeSet<_>>();
    if unique.len() != headers.len() {
        return Err(ConnectorError::InvalidStructure(
            "CSV headers must be unique",
        ));
    }
    Ok(headers
        .iter()
        .map(|header| header.trim().to_owned())
        .collect())
}

#[must_use]
pub fn source_sha256(bytes: &[u8]) -> String {
    sha256(bytes)
}

pub fn parse_tally_xml(bytes: &[u8], base_currency: CurrencyCode) -> Result<ParsedSource> {
    let source = source_text(bytes)?;
    let upper = source.to_ascii_uppercase();
    if upper.contains("<!DOCTYPE")
        || upper.contains("<!ENTITY")
        || upper.contains("<?XML")
        || upper.contains("<![CDATA[")
    {
        return Err(ConnectorError::UnsafeXml);
    }
    let envelopes = blocks(source, &upper, "ENVELOPE")?;
    if envelopes.len() != 1 || envelopes[0].trim() != source.trim() {
        return Err(ConnectorError::InvalidStructure(
            "Tally XML requires one ENVELOPE",
        ));
    }
    let source = envelopes[0];
    let upper = source.to_ascii_uppercase();
    let voucher_blocks = blocks(source, &upper, "VOUCHER")?;
    let (entity_kind, source_blocks) = if !voucher_blocks.is_empty() {
        (ConnectorEntityKind::Invoice, voucher_blocks)
    } else {
        let ledger_blocks = blocks(source, &upper, "LEDGER")?;
        if ledger_blocks.is_empty() {
            return Err(ConnectorError::InvalidStructure(
                "Tally XML contains no LEDGER or VOUCHER records",
            ));
        }
        (ConnectorEntityKind::Party, ledger_blocks)
    };
    if source_blocks.len() > MAX_SOURCE_ROWS {
        return Err(ConnectorError::InvalidStructure(
            "Tally XML record count exceeds the limit",
        ));
    }
    let rows = source_blocks
        .into_iter()
        .enumerate()
        .map(|(index, block)| {
            let row_number = u32::try_from(index + 1).unwrap_or(u32::MAX);
            match entity_kind {
                ConnectorEntityKind::Party => parse_tally_ledger(block, row_number),
                ConnectorEntityKind::Invoice => {
                    parse_tally_voucher(block, row_number, base_currency)
                }
            }
        })
        .collect();
    Ok(ParsedSource {
        sdk_version: CONNECTOR_SDK_VERSION,
        connector_key: TALLY_MANIFEST.connector_key.to_owned(),
        connector_version: TALLY_MANIFEST.version,
        source_sha256: sha256(bytes),
        entity_kind,
        mapping: FieldMapping {
            entity_kind,
            fields: BTreeMap::new(),
        },
        rows,
    })
}

fn parse_mapped_row<'a>(
    entity_kind: ConnectorEntityKind,
    row_number: u32,
    value: impl Fn(&str) -> Option<&'a str>,
) -> ParsedRow {
    match entity_kind {
        ConnectorEntityKind::Party => {
            let external_id = required(value("externalId"), "externalId");
            let legal_name = required(value("legalName"), "legalName");
            let kind = parse_party_kind(value("kind"));
            let errors = collect_errors([
                external_id.as_ref().err(),
                legal_name.as_ref().err(),
                kind.as_ref().err(),
            ]);
            let record = errors.is_empty().then(|| {
                CanonicalRecord::Party(CanonicalParty {
                    external_id: external_id.unwrap(),
                    legal_name: legal_name.unwrap(),
                    kind: kind.unwrap(),
                    tax_identifier: optional(value("taxIdentifier")),
                })
            });
            ParsedRow {
                row_number,
                record,
                errors,
            }
        }
        ConnectorEntityKind::Invoice => {
            let external_id = required(value("externalId"), "externalId");
            let party_name = required(value("partyName"), "partyName");
            let invoice_number = required(value("invoiceNumber"), "invoiceNumber");
            let invoice_date = parse_date(value("invoiceDate"), "invoiceDate", false);
            let due_date = parse_date(value("dueDate"), "dueDate", true);
            let total = parse_amount_minor(value("total"), "total").and_then(|amount| {
                if amount < 0 {
                    Err(row_error(
                        "total",
                        "negative_amount",
                        "Invoice total cannot be negative.",
                    ))
                } else {
                    Ok(amount)
                }
            });
            let currency = parse_currency(value("currency"));
            let direction = parse_direction(value("direction"));
            let errors = collect_errors([
                external_id.as_ref().err(),
                party_name.as_ref().err(),
                invoice_number.as_ref().err(),
                invoice_date.as_ref().err(),
                due_date.as_ref().err(),
                total.as_ref().err(),
                currency.as_ref().err(),
                direction.as_ref().err(),
            ]);
            let record = errors.is_empty().then(|| {
                CanonicalRecord::Invoice(CanonicalInvoice {
                    external_id: external_id.unwrap(),
                    party_external_id: optional(value("partyExternalId")),
                    party_name: party_name.unwrap(),
                    invoice_number: invoice_number.unwrap(),
                    invoice_date: invoice_date.unwrap().expect("required date validated"),
                    due_date: due_date.unwrap(),
                    total_minor_units: total.unwrap(),
                    currency: currency.unwrap(),
                    direction: direction.unwrap(),
                })
            });
            ParsedRow {
                row_number,
                record,
                errors,
            }
        }
    }
}

fn parse_tally_ledger(block: &str, row_number: u32) -> ParsedRow {
    let name = tag(block, "NAME").or_else(|| attribute(block, "NAME"));
    let parent = tag(block, "PARENT").unwrap_or_default();
    let kind = if parent.to_ascii_lowercase().contains("debtor") {
        PartyKind::Customer
    } else if parent.to_ascii_lowercase().contains("creditor") {
        PartyKind::Vendor
    } else {
        PartyKind::Both
    };
    match name.filter(|value| !value.trim().is_empty()) {
        Some(name) => ParsedRow {
            row_number,
            record: Some(CanonicalRecord::Party(CanonicalParty {
                external_id: tag(block, "GUID").unwrap_or_else(|| name.clone()),
                legal_name: name,
                kind,
                tax_identifier: tag(block, "PARTYGSTIN").filter(|value| !value.is_empty()),
            })),
            errors: vec![],
        },
        None => invalid_row(
            row_number,
            "legalName",
            "required",
            "Tally ledger has no name",
        ),
    }
}

fn parse_tally_voucher(block: &str, row_number: u32, base_currency: CurrencyCode) -> ParsedRow {
    let voucher_type = tag(block, "VOUCHERTYPENAME")
        .or_else(|| attribute(block, "VCHTYPE"))
        .unwrap_or_default();
    let direction = if voucher_type.to_ascii_lowercase().contains("sales") {
        Ok("receivable".to_owned())
    } else if voucher_type.to_ascii_lowercase().contains("purchase") {
        Ok("payable".to_owned())
    } else {
        Err(row_error(
            "direction",
            "unsupported_voucher_type",
            "Tally pilot supports purchase and sales vouchers only.",
        ))
    };
    let party = tag(block, "PARTYLEDGERNAME").unwrap_or_default();
    let number = tag(block, "VOUCHERNUMBER").unwrap_or_default();
    let date = tag(block, "DATE");
    let amount = tag(block, "AMOUNT");
    let date = parse_date(date.as_deref(), "invoiceDate", false);
    let total = parse_amount_minor(amount.as_deref(), "total").map(i64::abs);
    let party_value = required(Some(&party), "partyName");
    let number_value = required(Some(&number), "invoiceNumber");
    let errors = collect_errors([
        date.as_ref().err(),
        total.as_ref().err(),
        party_value.as_ref().err(),
        number_value.as_ref().err(),
        direction.as_ref().err(),
    ]);
    let record = errors.is_empty().then(|| {
        let date_value = date.unwrap().expect("required date validated");
        let external_id = tag(block, "GUID")
            .or_else(|| attribute(block, "REMOTEID"))
            .unwrap_or_else(|| {
                format!(
                    "{}|{}|{}|{}",
                    number,
                    date_value,
                    party,
                    total.as_ref().unwrap()
                )
            });
        CanonicalRecord::Invoice(CanonicalInvoice {
            external_id,
            party_external_id: None,
            party_name: party,
            invoice_number: number,
            invoice_date: date_value,
            due_date: tag(block, "BASICDUEDATEOFPMT").and_then(|value| normalize_date(&value).ok()),
            total_minor_units: total.unwrap(),
            currency: base_currency,
            direction: direction.unwrap(),
        })
    });
    ParsedRow {
        row_number,
        record,
        errors,
    }
}

fn source_text(bytes: &[u8]) -> Result<&str> {
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(ConnectorError::SourceTooLarge);
    }
    let source = std::str::from_utf8(bytes).map_err(|_| ConnectorError::InvalidUtf8)?;
    if source.starts_with('\u{feff}') || source.contains('\0') {
        return Err(ConnectorError::InvalidUtf8);
    }
    Ok(source)
}

fn parse_csv_rows(source: &str) -> Result<Vec<Vec<String>>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut chars = source.chars().peekable();
    let mut quoted = false;
    while let Some(character) = chars.next() {
        if quoted {
            if character == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cell.push('"');
                } else {
                    quoted = false;
                }
            } else {
                cell.push(character);
            }
        } else {
            match character {
                '"' if cell.is_empty() => quoted = true,
                '"' => {
                    return Err(ConnectorError::InvalidStructure(
                        "quote appears inside an unquoted CSV cell",
                    ))
                }
                ',' => {
                    row.push(take_cell(&mut cell)?);
                }
                '\n' => {
                    row.push(take_cell(&mut cell)?);
                    push_row(&mut rows, &mut row)?;
                }
                '\r' if chars.peek() == Some(&'\n') => {}
                '\r' => {
                    row.push(take_cell(&mut cell)?);
                    push_row(&mut rows, &mut row)?;
                }
                _ => cell.push(character),
            }
        }
    }
    if quoted {
        return Err(ConnectorError::InvalidStructure(
            "CSV quoted field is not closed",
        ));
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(take_cell(&mut cell)?);
        push_row(&mut rows, &mut row)?;
    }
    if rows.len() > MAX_SOURCE_ROWS + 1 {
        return Err(ConnectorError::InvalidStructure(
            "CSV row count exceeds the limit",
        ));
    }
    Ok(rows)
}

fn take_cell(cell: &mut String) -> Result<String> {
    if cell.len() > MAX_CELL_BYTES {
        return Err(ConnectorError::InvalidStructure(
            "CSV cell exceeds the limit",
        ));
    }
    Ok(std::mem::take(cell))
}

fn push_row(rows: &mut Vec<Vec<String>>, row: &mut Vec<String>) -> Result<()> {
    if row.len() > MAX_COLUMNS {
        return Err(ConnectorError::InvalidStructure(
            "CSV column count exceeds the limit",
        ));
    }
    if row.iter().any(|cell| !cell.is_empty()) {
        rows.push(std::mem::take(row));
    } else {
        row.clear();
    }
    Ok(())
}

fn validate_mapping(mapping: &FieldMapping, headers: &[String]) -> Result<()> {
    let known = canonical_fields(mapping.entity_kind);
    if mapping
        .fields
        .keys()
        .any(|field| !known.contains(field.as_str()))
    {
        return Err(ConnectorError::InvalidMapping("unknown canonical field"));
    }
    let header_set = headers
        .iter()
        .map(|header| header.trim())
        .collect::<BTreeSet<_>>();
    if mapping
        .fields
        .values()
        .any(|column| !header_set.contains(column.trim()))
    {
        return Err(ConnectorError::InvalidMapping(
            "mapped source column is missing",
        ));
    }
    let required_fields: &[&str] = match mapping.entity_kind {
        ConnectorEntityKind::Party => &["externalId", "legalName", "kind"],
        ConnectorEntityKind::Invoice => &[
            "externalId",
            "partyName",
            "invoiceNumber",
            "invoiceDate",
            "total",
            "currency",
            "direction",
        ],
    };
    if required_fields
        .iter()
        .any(|field| !mapping.fields.contains_key(*field))
    {
        return Err(ConnectorError::InvalidMapping(
            "required canonical field is not mapped",
        ));
    }
    Ok(())
}

fn canonical_fields(kind: ConnectorEntityKind) -> BTreeSet<&'static str> {
    let fields: &[&str] = match kind {
        ConnectorEntityKind::Party => &["externalId", "legalName", "kind", "taxIdentifier"],
        ConnectorEntityKind::Invoice => &[
            "externalId",
            "partyExternalId",
            "partyName",
            "invoiceNumber",
            "invoiceDate",
            "dueDate",
            "total",
            "currency",
            "direction",
        ],
    };
    fields.iter().copied().collect()
}

fn required(value: Option<&str>, field: &str) -> std::result::Result<String, RowError> {
    let value = value.unwrap_or_default().trim();
    if value.is_empty() {
        return Err(row_error(field, "required", "Required value is missing."));
    }
    if value.len() > 240 {
        return Err(row_error(
            field,
            "too_long",
            "Value exceeds 240 characters.",
        ));
    }
    Ok(value.to_owned())
}

fn optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(240).collect())
}

fn parse_party_kind(value: Option<&str>) -> std::result::Result<PartyKind, RowError> {
    match value
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "vendor" | "supplier" | "sundry creditor" => Ok(PartyKind::Vendor),
        "customer" | "sundry debtor" => Ok(PartyKind::Customer),
        "both" => Ok(PartyKind::Both),
        _ => Err(row_error(
            "kind",
            "invalid_enum",
            "Kind must be vendor, customer, or both.",
        )),
    }
}

fn parse_direction(value: Option<&str>) -> std::result::Result<String, RowError> {
    match value
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "payable" | "purchase" => Ok("payable".to_owned()),
        "receivable" | "sales" => Ok("receivable".to_owned()),
        _ => Err(row_error(
            "direction",
            "invalid_enum",
            "Direction must be payable or receivable.",
        )),
    }
}

fn parse_date(
    value: Option<&str>,
    field: &str,
    optional: bool,
) -> std::result::Result<Option<LocalDate>, RowError> {
    let value = value.unwrap_or_default().trim();
    if value.is_empty() && optional {
        return Ok(None);
    }
    normalize_date(value).map(Some).map_err(|_| {
        row_error(
            field,
            "invalid_date",
            "Date must be YYYY-MM-DD or YYYYMMDD.",
        )
    })
}

fn normalize_date(value: &str) -> std::result::Result<LocalDate, ()> {
    let normalized = if value.len() == 8 && value.bytes().all(|byte| byte.is_ascii_digit()) {
        format!("{}-{}-{}", &value[..4], &value[4..6], &value[6..8])
    } else {
        value.to_owned()
    };
    normalized.parse().map_err(|_| ())
}

fn parse_currency(value: Option<&str>) -> std::result::Result<CurrencyCode, RowError> {
    CurrencyCode::parse(value.unwrap_or_default().trim()).map_err(|_| {
        row_error(
            "currency",
            "invalid_currency",
            "Currency must be three uppercase letters.",
        )
    })
}

fn parse_amount_minor(value: Option<&str>, field: &str) -> std::result::Result<i64, RowError> {
    let value = value.unwrap_or_default().trim();
    if value.is_empty() {
        return Err(row_error(field, "required", "Amount is required."));
    }
    let negative = value.starts_with('-');
    let unsigned = value.strip_prefix('-').unwrap_or(value);
    let mut parts = unsigned.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next().unwrap_or_default();
    if parts.next().is_some()
        || whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > 2
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(row_error(
            field,
            "invalid_amount",
            "Amount must use at most two decimal places and no grouping separators.",
        ));
    }
    let whole = whole.parse::<i64>().map_err(|_| {
        row_error(
            field,
            "amount_overflow",
            "Amount exceeds the supported range.",
        )
    })?;
    let fraction = format!("{fraction:0<2}")
        .parse::<i64>()
        .map_err(|_| row_error(field, "invalid_amount", "Amount is invalid."))?;
    let minor = whole
        .checked_mul(100)
        .and_then(|value| value.checked_add(fraction))
        .ok_or_else(|| {
            row_error(
                field,
                "amount_overflow",
                "Amount exceeds the supported range.",
            )
        })?;
    Ok(if negative { -minor } else { minor })
}

fn collect_errors<const N: usize>(errors: [Option<&RowError>; N]) -> Vec<RowError> {
    errors.into_iter().flatten().cloned().collect()
}

fn row_error(field: &str, code: &str, message: &str) -> RowError {
    RowError {
        field: field.to_owned(),
        code: code.to_owned(),
        message: message.to_owned(),
    }
}

fn invalid_row(row_number: u32, field: &str, code: &str, message: &str) -> ParsedRow {
    ParsedRow {
        row_number,
        record: None,
        errors: vec![row_error(field, code, message)],
    }
}

fn blocks<'a>(source: &'a str, upper: &str, tag_name: &str) -> Result<Vec<&'a str>> {
    let start_pattern = format!("<{tag_name}");
    let end_pattern = format!("</{tag_name}>");
    let mut cursor = 0;
    let mut found = Vec::new();
    while let Some(start) = find_open_tag(upper, &start_pattern, cursor) {
        let Some(open_end_relative) = upper[start..].find('>') else {
            return Err(ConnectorError::InvalidStructure(
                "XML start tag is not closed",
            ));
        };
        let open_end = start + open_end_relative + 1;
        let Some(close_relative) = upper[open_end..].find(&end_pattern) else {
            return Err(ConnectorError::InvalidStructure(
                "XML record tag is not closed",
            ));
        };
        let end = open_end + close_relative + end_pattern.len();
        found.push(&source[start..end]);
        cursor = end;
    }
    Ok(found)
}

fn tag(source: &str, name: &str) -> Option<String> {
    let upper = source.to_ascii_uppercase();
    let start_pattern = format!("<{name}");
    let start = find_open_tag(&upper, &start_pattern, 0)?;
    let content_start = start + upper[start..].find('>')? + 1;
    let end_pattern = format!("</{name}>");
    let content_end = content_start + upper[content_start..].find(&end_pattern)?;
    decode_xml(source[content_start..content_end].trim()).ok()
}

fn find_open_tag(source: &str, pattern: &str, mut cursor: usize) -> Option<usize> {
    while let Some(relative_start) = source[cursor..].find(pattern) {
        let start = cursor + relative_start;
        let boundary = source.as_bytes().get(start + pattern.len()).copied()?;
        if boundary == b'>' || boundary.is_ascii_whitespace() {
            return Some(start);
        }
        cursor = start + pattern.len();
    }
    None
}

fn attribute(source: &str, name: &str) -> Option<String> {
    let open_end = source.find('>')?;
    let open = &source[..open_end];
    let upper = open.to_ascii_uppercase();
    let pattern = format!("{name}=\"");
    let start = upper.find(&pattern)? + pattern.len();
    let end = start + open[start..].find('"')?;
    decode_xml(&open[start..end]).ok()
}

fn decode_xml(value: &str) -> Result<String> {
    let mut output = String::new();
    let mut rest = value;
    while let Some(index) = rest.find('&') {
        output.push_str(&rest[..index]);
        rest = &rest[index..];
        let end = rest.find(';').ok_or(ConnectorError::UnsafeXml)?;
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            value if value.starts_with("#x") => char::from_u32(
                u32::from_str_radix(&value[2..], 16).map_err(|_| ConnectorError::UnsafeXml)?,
            )
            .ok_or(ConnectorError::UnsafeXml)?,
            value if value.starts_with('#') => {
                char::from_u32(value[1..].parse().map_err(|_| ConnectorError::UnsafeXml)?)
                    .ok_or(ConnectorError::UnsafeXml)?
            }
            _ => return Err(ConnectorError::UnsafeXml),
        };
        if decoded.is_control() && !matches!(decoded, '\n' | '\r' | '\t') {
            return Err(ConnectorError::UnsafeXml);
        }
        output.push(decoded);
        rest = &rest[end + 1..];
    }
    output.push_str(rest);
    Ok(output)
}

#[must_use]
pub fn csv_escape(value: &str) -> String {
    let neutralized = if value.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{value}")
    } else {
        value.to_owned()
    };
    if neutralized.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", neutralized.replace('"', "\"\""))
    } else {
        neutralized
    }
}

fn sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    fn party_mapping() -> FieldMapping {
        FieldMapping {
            entity_kind: ConnectorEntityKind::Party,
            fields: BTreeMap::from([
                ("externalId".to_owned(), "Code".to_owned()),
                ("legalName".to_owned(), "Name".to_owned()),
                ("kind".to_owned(), "Kind".to_owned()),
                ("taxIdentifier".to_owned(), "GSTIN".to_owned()),
            ]),
        }
    }

    #[test]
    fn csv_accepts_valid_rows_and_keeps_row_errors_independent() {
        let parsed = parse_csv(
            b"Code,Name,Kind,GSTIN\nV1,Alpha Pvt Ltd,vendor,29ABCDE1234F1Z5\nV2,Beta,unknown,\n",
            party_mapping(),
        )
        .unwrap();
        assert_eq!(parsed.valid_count(), 1);
        assert_eq!(parsed.invalid_count(), 1);
        assert_eq!(parsed.rows[1].errors[0].field, "kind");
    }

    #[test]
    fn csv_handles_quotes_newlines_and_neutralizes_export_formulas() {
        let parsed = parse_csv(
            b"Code,Name,Kind,GSTIN\nV1,\"Alpha, \"\"Works\"\"\",vendor,\n",
            party_mapping(),
        )
        .unwrap();
        let Some(CanonicalRecord::Party(party)) = &parsed.rows[0].record else {
            panic!("party expected")
        };
        assert_eq!(party.legal_name, "Alpha, \"Works\"");
        assert_eq!(csv_escape("=2+2"), "'=2+2");
        assert_eq!(csv_escape("A,B"), "\"A,B\"");
    }

    #[test]
    fn tally_xml_parses_ledgers_and_vouchers_without_network_or_credentials() {
        let ledgers = br#"<ENVELOPE><BODY><DATA><TALLYMESSAGE><LEDGER NAME="Acme &amp; Co"><GUID>L-1</GUID><PARENT>Sundry Creditors</PARENT><PARTYGSTIN>29ABCDE1234F1Z5</PARTYGSTIN></LEDGER></TALLYMESSAGE></DATA></BODY></ENVELOPE>"#;
        let parsed = parse_tally_xml(ledgers, CurrencyCode::parse("INR").unwrap()).unwrap();
        assert_eq!(parsed.entity_kind, ConnectorEntityKind::Party);
        assert_eq!(parsed.valid_count(), 1);
        let vouchers = br#"<ENVELOPE><BODY><DATA><TALLYMESSAGE><VOUCHER REMOTEID="V-1" VCHTYPE="Purchase"><DATE>20260805</DATE><VOUCHERNUMBER>P-42</VOUCHERNUMBER><PARTYLEDGERNAME>Acme</PARTYLEDGERNAME><AMOUNT>-1234.56</AMOUNT></VOUCHER></TALLYMESSAGE></DATA></BODY></ENVELOPE>"#;
        let parsed = parse_tally_xml(vouchers, CurrencyCode::parse("INR").unwrap()).unwrap();
        let Some(CanonicalRecord::Invoice(invoice)) = &parsed.rows[0].record else {
            panic!("invoice expected")
        };
        assert_eq!(invoice.total_minor_units, 123_456);
        assert_eq!(invoice.direction, "payable");
    }

    #[test]
    fn tally_voucher_nested_ledger_entries_are_not_misread_as_ledger_masters() {
        let xml = br#"<ENVELOPE><BODY><DATA><TALLYMESSAGE>
            <VOUCHER REMOTEID="v-1" VCHTYPE="Purchase">
              <DATE>20260801</DATE><VOUCHERNUMBER>INV-1</VOUCHERNUMBER>
              <PARTYLEDGERNAME>Acme</PARTYLEDGERNAME>
              <ALLLEDGERENTRIES.LIST><AMOUNT>-1250.00</AMOUNT></ALLLEDGERENTRIES.LIST>
            </VOUCHER>
          </TALLYMESSAGE></DATA></BODY></ENVELOPE>"#;
        let parsed = parse_tally_xml(xml, CurrencyCode::parse("INR").unwrap()).unwrap();

        assert_eq!(parsed.entity_kind, ConnectorEntityKind::Invoice);
        assert_eq!(parsed.rows.len(), 1);
        assert!(parsed.rows[0].errors.is_empty());
    }

    #[test]
    fn tally_pilot_rejects_unsupported_voucher_types_per_row() {
        let xml = br#"<ENVELOPE><BODY><DATA><TALLYMESSAGE>
            <VOUCHER REMOTEID="v-2" VCHTYPE="Journal">
              <DATE>20260801</DATE><VOUCHERNUMBER>J-1</VOUCHERNUMBER>
              <PARTYLEDGERNAME>Acme</PARTYLEDGERNAME><AMOUNT>1250.00</AMOUNT>
            </VOUCHER>
          </TALLYMESSAGE></DATA></BODY></ENVELOPE>"#;
        let parsed = parse_tally_xml(xml, CurrencyCode::parse("INR").unwrap()).unwrap();

        assert_eq!(parsed.invalid_count(), 1);
        assert_eq!(parsed.rows[0].errors[0].code, "unsupported_voucher_type");
    }

    #[test]
    fn synthetic_connector_fixtures_match_contract_v1() {
        let mapping = FieldMapping {
            entity_kind: ConnectorEntityKind::Party,
            fields: BTreeMap::from([
                ("externalId".to_owned(), "Code".to_owned()),
                ("legalName".to_owned(), "Name".to_owned()),
                ("kind".to_owned(), "Type".to_owned()),
                ("taxIdentifier".to_owned(), "GSTIN".to_owned()),
            ]),
        };
        let parties = parse_csv(
            include_bytes!("../../../fixtures/connectors/parties-valid-invalid.csv"),
            mapping,
        )
        .unwrap();
        let ledgers = parse_tally_xml(
            include_bytes!("../../../fixtures/connectors/tally-ledgers.xml"),
            CurrencyCode::parse("INR").unwrap(),
        )
        .unwrap();
        let vouchers = parse_tally_xml(
            include_bytes!("../../../fixtures/connectors/tally-vouchers.xml"),
            CurrencyCode::parse("INR").unwrap(),
        )
        .unwrap();

        assert_eq!((parties.valid_count(), parties.invalid_count()), (2, 1));
        assert_eq!(ledgers.valid_count(), 2);
        assert_eq!(vouchers.valid_count(), 1);
    }

    #[test]
    fn tally_xml_rejects_entities_doctype_and_unbounded_inputs() {
        assert!(matches!(
            parse_tally_xml(
                b"<!DOCTYPE x><ENVELOPE></ENVELOPE>",
                CurrencyCode::parse("INR").unwrap()
            ),
            Err(ConnectorError::UnsafeXml)
        ));
        assert!(matches!(
            parse_csv(&vec![b'x'; MAX_SOURCE_BYTES + 1], party_mapping()),
            Err(ConnectorError::SourceTooLarge)
        ));
        assert!(matches!(
            parse_tally_xml(
                b"<ENVELOPE></ENVELOPE><ENVELOPE></ENVELOPE>",
                CurrencyCode::parse("INR").unwrap()
            ),
            Err(ConnectorError::InvalidStructure(_))
        ));
    }
}
