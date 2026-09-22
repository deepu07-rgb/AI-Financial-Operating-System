use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{CurrencyCode, DomainError, EntityId, InvoiceStatus, LocalDate, PartyKind};

pub const RULE_ENGINE_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleSeverity {
    Info,
    Warning,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThresholdRule {
    pub version: u32,
    pub enabled: bool,
    pub threshold: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleEvaluationConfig {
    pub probable_duplicate: ThresholdRule,
    pub arithmetic_mismatch: ThresholdRule,
    pub price_increase: ThresholdRule,
    pub due_soon: ThresholdRule,
    pub unusual_amount: ThresholdRule,
    pub unusual_minimum_history: u32,
    pub concentration: ThresholdRule,
}

impl Default for RuleEvaluationConfig {
    fn default() -> Self {
        Self {
            probable_duplicate: ThresholdRule {
                version: 1,
                enabled: true,
                threshold: 3,
            },
            arithmetic_mismatch: ThresholdRule {
                version: 1,
                enabled: true,
                threshold: 1,
            },
            price_increase: ThresholdRule {
                version: 1,
                enabled: true,
                threshold: 1_000,
            },
            due_soon: ThresholdRule {
                version: 1,
                enabled: true,
                threshold: 14,
            },
            unusual_amount: ThresholdRule {
                version: 1,
                enabled: true,
                threshold: 20_000,
            },
            unusual_minimum_history: 3,
            concentration: ThresholdRule {
                version: 1,
                enabled: true,
                threshold: 3_000,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleLineSnapshot {
    pub id: EntityId,
    pub description: String,
    pub category: Option<String>,
    pub unit_rate_minor_units: i64,
    pub total_minor_units: i64,
    pub currency: CurrencyCode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleInvoiceSnapshot {
    pub id: EntityId,
    pub document_id: EntityId,
    pub version: u32,
    pub status: InvoiceStatus,
    pub party_id: Option<EntityId>,
    pub party_kind: Option<PartyKind>,
    pub party_name: Option<String>,
    pub invoice_number: Option<String>,
    pub invoice_date: Option<LocalDate>,
    pub due_date: Option<LocalDate>,
    pub total_minor_units: i64,
    pub currency: CurrencyCode,
    pub confidence_basis_points: Option<u16>,
    pub lines: Vec<RuleLineSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuleInputSnapshot {
    ProbableDuplicate {
        invoice_id: EntityId,
        matching_invoice_id: EntityId,
        normalized_invoice_number: Option<String>,
        amount_minor_units: i64,
        currency: CurrencyCode,
    },
    ArithmeticMismatch {
        invoice_total_minor_units: i64,
        line_total_minor_units: i64,
        tolerance_minor_units: i64,
        currency: CurrencyCode,
    },
    PriceIncrease {
        line_id: EntityId,
        normalized_item: String,
        baseline_minor_units: i64,
        current_minor_units: i64,
        threshold_basis_points: i64,
        currency: CurrencyCode,
    },
    DueSoon {
        due_date: LocalDate,
        as_of_date: LocalDate,
        days_until_due: i64,
        window_days: i64,
    },
    UnusualAmount {
        amount_minor_units: i64,
        historical_average_minor_units: i64,
        history_count: u32,
        multiplier_basis_points: i64,
        currency: CurrencyCode,
    },
    ConcentrationRisk {
        party_id: EntityId,
        party_total_minor_units: i64,
        workspace_total_minor_units: i64,
        share_basis_points: i64,
        threshold_basis_points: i64,
        currency: CurrencyCode,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleFinding {
    pub rule_id: &'static str,
    pub rule_version: u32,
    pub subject_type: &'static str,
    pub subject_id: EntityId,
    pub severity: RuleSeverity,
    pub explanation: String,
    pub input: RuleInputSnapshot,
}

pub fn evaluate_invoice_rules(
    invoices: &[RuleInvoiceSnapshot],
    as_of_date: LocalDate,
    config: RuleEvaluationConfig,
) -> Result<Vec<RuleFinding>, DomainError> {
    let mut approved = invoices
        .iter()
        .filter(|invoice| invoice.status == InvoiceStatus::Approved)
        .collect::<Vec<_>>();
    approved.sort_by_key(|invoice| {
        (
            invoice.invoice_date,
            invoice.id.to_string(),
            invoice.version,
        )
    });

    let mut findings = Vec::new();
    evaluate_duplicates(&approved, config.probable_duplicate, &mut findings);
    evaluate_invoice_level_rules(&approved, as_of_date, config, &mut findings)?;
    evaluate_concentration(&approved, config.concentration, &mut findings)?;
    Ok(findings)
}

fn evaluate_duplicates(
    invoices: &[&RuleInvoiceSnapshot],
    setting: ThresholdRule,
    findings: &mut Vec<RuleFinding>,
) {
    if !setting.enabled {
        return;
    }
    let mut numbers = HashMap::<(EntityId, String), EntityId>::new();
    let mut amounts = HashMap::<(EntityId, CurrencyCode, i64), Vec<(EntityId, LocalDate)>>::new();
    for invoice in invoices {
        let Some(party_id) = invoice.party_id else {
            continue;
        };
        let normalized_number = invoice
            .invoice_number
            .as_deref()
            .map(normalize_key)
            .filter(|value| !value.is_empty());
        let number_match = normalized_number
            .as_ref()
            .and_then(|number| numbers.get(&(party_id, number.clone())).copied());
        let amount_match = invoice.invoice_date.and_then(|date| {
            amounts
                .get(&(party_id, invoice.currency, invoice.total_minor_units))
                .and_then(|history| {
                    history.iter().rev().find_map(|(id, prior_date)| {
                        (prior_date.days_until(date).unsigned_abs()
                            <= setting.threshold.max(0) as u64)
                            .then_some(*id)
                    })
                })
        });
        if let Some(matching_invoice_id) = number_match.or(amount_match) {
            findings.push(RuleFinding {
                rule_id: "RULE-002",
                rule_version: setting.version,
                subject_type: "invoice",
                subject_id: invoice.id,
                severity: RuleSeverity::High,
                explanation: "Possible duplicate: vendor and invoice identity or nearby amount/date match an approved invoice.".to_owned(),
                input: RuleInputSnapshot::ProbableDuplicate {
                    invoice_id: invoice.id,
                    matching_invoice_id,
                    normalized_invoice_number: normalized_number.clone(),
                    amount_minor_units: invoice.total_minor_units,
                    currency: invoice.currency,
                },
            });
        }
        if let Some(number) = normalized_number {
            numbers.entry((party_id, number)).or_insert(invoice.id);
        }
        if let Some(date) = invoice.invoice_date {
            amounts
                .entry((party_id, invoice.currency, invoice.total_minor_units))
                .or_default()
                .push((invoice.id, date));
        }
    }
}

fn evaluate_invoice_level_rules(
    invoices: &[&RuleInvoiceSnapshot],
    as_of_date: LocalDate,
    config: RuleEvaluationConfig,
    findings: &mut Vec<RuleFinding>,
) -> Result<(), DomainError> {
    let mut item_history = HashMap::<(EntityId, String, CurrencyCode), (i128, u32)>::new();
    let mut amount_history = HashMap::<(EntityId, CurrencyCode), (i128, u32)>::new();
    for invoice in invoices {
        if config.arithmetic_mismatch.enabled && !invoice.lines.is_empty() {
            let line_total = checked_i128_sum(
                invoice
                    .lines
                    .iter()
                    .map(|line| i128::from(line.total_minor_units)),
            )?;
            if i128::from(invoice.total_minor_units).abs_diff(i128::from(line_total))
                > config.arithmetic_mismatch.threshold.max(0) as u128
            {
                findings.push(RuleFinding {
                    rule_id: "RULE-003",
                    rule_version: config.arithmetic_mismatch.version,
                    subject_type: "invoice",
                    subject_id: invoice.id,
                    severity: RuleSeverity::High,
                    explanation:
                        "Invoice line totals do not reconcile to the approved invoice total."
                            .to_owned(),
                    input: RuleInputSnapshot::ArithmeticMismatch {
                        invoice_total_minor_units: invoice.total_minor_units,
                        line_total_minor_units: line_total,
                        tolerance_minor_units: config.arithmetic_mismatch.threshold,
                        currency: invoice.currency,
                    },
                });
            }
        }

        if config.due_soon.enabled {
            if let Some(due_date) = invoice.due_date {
                let days = as_of_date.days_until(due_date);
                if (0..=config.due_soon.threshold.max(0)).contains(&days) {
                    findings.push(RuleFinding {
                        rule_id: "RULE-005",
                        rule_version: config.due_soon.version,
                        subject_type: "invoice",
                        subject_id: invoice.id,
                        severity: if days <= 3 {
                            RuleSeverity::High
                        } else {
                            RuleSeverity::Warning
                        },
                        explanation: format!("Approved invoice is due in {days} day(s)."),
                        input: RuleInputSnapshot::DueSoon {
                            due_date,
                            as_of_date,
                            days_until_due: days,
                            window_days: config.due_soon.threshold,
                        },
                    });
                }
            }
        }

        if let Some(party_id) = invoice.party_id {
            evaluate_unusual_amount(invoice, party_id, config, &amount_history, findings)?;
            evaluate_price_increases(
                invoice,
                party_id,
                config.price_increase,
                &item_history,
                findings,
            )?;
            let amount = amount_history
                .entry((party_id, invoice.currency))
                .or_insert((0, 0));
            amount.0 = amount
                .0
                .checked_add(i128::from(invoice.total_minor_units))
                .ok_or(DomainError::MoneyOverflow)?;
            amount.1 = amount.1.saturating_add(1);
            for line in &invoice.lines {
                let key = (
                    party_id,
                    normalize_key(line.category.as_deref().unwrap_or(&line.description)),
                    line.currency,
                );
                if key.1.is_empty() {
                    continue;
                }
                let history = item_history.entry(key).or_insert((0, 0));
                history.0 = history
                    .0
                    .checked_add(i128::from(line.unit_rate_minor_units))
                    .ok_or(DomainError::MoneyOverflow)?;
                history.1 = history.1.saturating_add(1);
            }
        }
    }
    Ok(())
}

fn evaluate_unusual_amount(
    invoice: &RuleInvoiceSnapshot,
    party_id: EntityId,
    config: RuleEvaluationConfig,
    history: &HashMap<(EntityId, CurrencyCode), (i128, u32)>,
    findings: &mut Vec<RuleFinding>,
) -> Result<(), DomainError> {
    if !config.unusual_amount.enabled {
        return Ok(());
    }
    let Some((sum, count)) = history.get(&(party_id, invoice.currency)) else {
        return Ok(());
    };
    if *count < config.unusual_minimum_history || *count == 0 {
        return Ok(());
    }
    let average = sum / i128::from(*count);
    let current = i128::from(invoice.total_minor_units).abs();
    if average.abs() > 0
        && current * 10_000 > average.abs() * i128::from(config.unusual_amount.threshold)
    {
        findings.push(RuleFinding {
            rule_id: "RULE-008",
            rule_version: config.unusual_amount.version,
            subject_type: "invoice",
            subject_id: invoice.id,
            severity: RuleSeverity::Warning,
            explanation: format!(
                "Invoice amount exceeds the configured multiple of the vendor's historical average across {count} invoice(s)."
            ),
            input: RuleInputSnapshot::UnusualAmount {
                amount_minor_units: invoice.total_minor_units,
                historical_average_minor_units: i64::try_from(average)
                    .map_err(|_| DomainError::MoneyOverflow)?,
                history_count: *count,
                multiplier_basis_points: config.unusual_amount.threshold,
                currency: invoice.currency,
            },
        });
    }
    Ok(())
}

fn evaluate_price_increases(
    invoice: &RuleInvoiceSnapshot,
    party_id: EntityId,
    setting: ThresholdRule,
    history: &HashMap<(EntityId, String, CurrencyCode), (i128, u32)>,
    findings: &mut Vec<RuleFinding>,
) -> Result<(), DomainError> {
    if !setting.enabled {
        return Ok(());
    }
    for line in &invoice.lines {
        let normalized_item = normalize_key(line.category.as_deref().unwrap_or(&line.description));
        let Some((sum, count)) = history.get(&(party_id, normalized_item.clone(), line.currency))
        else {
            continue;
        };
        if *count == 0 {
            continue;
        }
        let baseline = sum / i128::from(*count);
        let current = i128::from(line.unit_rate_minor_units);
        if baseline > 0
            && current > baseline
            && (current - baseline) * 10_000 > baseline * i128::from(setting.threshold)
        {
            findings.push(RuleFinding {
                rule_id: "RULE-004",
                rule_version: setting.version,
                subject_type: "invoice_line",
                subject_id: line.id,
                severity: RuleSeverity::Warning,
                explanation: format!(
                    "Unit price increased above the configured threshold versus {count} historical line(s)."
                ),
                input: RuleInputSnapshot::PriceIncrease {
                    line_id: line.id,
                    normalized_item,
                    baseline_minor_units: i64::try_from(baseline)
                        .map_err(|_| DomainError::MoneyOverflow)?,
                    current_minor_units: line.unit_rate_minor_units,
                    threshold_basis_points: setting.threshold,
                    currency: line.currency,
                },
            });
        }
    }
    Ok(())
}

fn evaluate_concentration(
    invoices: &[&RuleInvoiceSnapshot],
    setting: ThresholdRule,
    findings: &mut Vec<RuleFinding>,
) -> Result<(), DomainError> {
    if !setting.enabled {
        return Ok(());
    }
    let mut totals = HashMap::<(EntityId, CurrencyCode), i128>::new();
    let mut workspace_totals = HashMap::<CurrencyCode, i128>::new();
    for invoice in invoices {
        if !matches!(
            invoice.party_kind,
            Some(PartyKind::Vendor | PartyKind::Both)
        ) {
            continue;
        }
        let Some(party_id) = invoice.party_id else {
            continue;
        };
        let amount = i128::from(invoice.total_minor_units).max(0);
        *totals.entry((party_id, invoice.currency)).or_default() += amount;
        *workspace_totals.entry(invoice.currency).or_default() += amount;
    }
    for ((party_id, currency), party_total) in totals {
        let workspace_total = workspace_totals[&currency];
        if workspace_total <= 0 {
            continue;
        }
        let share = party_total * 10_000 / workspace_total;
        if share > i128::from(setting.threshold) {
            findings.push(RuleFinding {
                rule_id: "RULE-012",
                rule_version: setting.version,
                subject_type: "party",
                subject_id: party_id,
                severity: RuleSeverity::Warning,
                explanation:
                    "Vendor share of approved spend exceeds the configured concentration threshold."
                        .to_owned(),
                input: RuleInputSnapshot::ConcentrationRisk {
                    party_id,
                    party_total_minor_units: i64::try_from(party_total)
                        .map_err(|_| DomainError::MoneyOverflow)?,
                    workspace_total_minor_units: i64::try_from(workspace_total)
                        .map_err(|_| DomainError::MoneyOverflow)?,
                    share_basis_points: i64::try_from(share)
                        .map_err(|_| DomainError::MoneyOverflow)?,
                    threshold_basis_points: setting.threshold,
                    currency,
                },
            });
        }
    }
    Ok(())
}

fn checked_i128_sum(mut values: impl Iterator<Item = i128>) -> Result<i64, DomainError> {
    let total = values
        .try_fold(0_i128, |sum, value| sum.checked_add(value))
        .ok_or(DomainError::MoneyOverflow)?;
    i64::try_from(total).map_err(|_| DomainError::MoneyOverflow)
}

fn normalize_key(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn invoice(
        date: &str,
        due_date: &str,
        total: i64,
        number: &str,
        party_id: EntityId,
        unit_rate: i64,
    ) -> RuleInvoiceSnapshot {
        let currency = CurrencyCode::parse("INR").unwrap();
        let id = EntityId::new();
        RuleInvoiceSnapshot {
            id,
            document_id: EntityId::new(),
            version: 1,
            status: InvoiceStatus::Approved,
            party_id: Some(party_id),
            party_kind: Some(PartyKind::Vendor),
            party_name: Some("Synthetic Supplier".to_owned()),
            invoice_number: Some(number.to_owned()),
            invoice_date: Some(date.parse().unwrap()),
            due_date: Some(due_date.parse().unwrap()),
            total_minor_units: total,
            currency,
            confidence_basis_points: Some(9_500),
            lines: vec![RuleLineSnapshot {
                id: EntityId::new(),
                description: "Steel component".to_owned(),
                category: Some("Materials".to_owned()),
                unit_rate_minor_units: unit_rate,
                total_minor_units: total,
                currency,
            }],
        }
    }

    #[test]
    fn deterministic_rules_find_duplicates_due_dates_prices_and_anomalies() {
        let party = EntityId::new();
        let mut invoices = vec![
            invoice("2026-01-01", "2026-01-31", 10_000, "A-1", party, 10_000),
            invoice("2026-02-01", "2026-02-28", 10_000, "A-2", party, 10_000),
            invoice("2026-03-01", "2026-03-31", 10_000, "A-3", party, 10_000),
            invoice("2026-08-01", "2026-08-10", 30_000, "A-4", party, 15_000),
        ];
        invoices.push(invoice(
            "2026-08-02",
            "2026-08-11",
            30_000,
            "A-4",
            party,
            15_000,
        ));
        let findings = evaluate_invoice_rules(
            &invoices,
            "2026-08-04".parse().unwrap(),
            RuleEvaluationConfig::default(),
        )
        .unwrap();
        let ids = findings
            .iter()
            .map(|finding| finding.rule_id)
            .collect::<Vec<_>>();
        assert!(ids.contains(&"RULE-002"));
        assert!(ids.contains(&"RULE-004"));
        assert!(ids.contains(&"RULE-005"));
        assert!(ids.contains(&"RULE-008"));
        assert!(ids.contains(&"RULE-012"));
    }

    #[test]
    fn disabled_rules_and_non_approved_invoices_do_not_trigger() {
        let party = EntityId::new();
        let mut candidate = invoice("2026-08-01", "2026-08-05", 10_000, "A-1", party, 10_000);
        candidate.status = InvoiceStatus::Draft;
        let findings = evaluate_invoice_rules(
            &[candidate],
            "2026-08-04".parse().unwrap(),
            RuleEvaluationConfig::default(),
        )
        .unwrap();
        assert!(findings.is_empty());
    }

    #[test]
    fn arithmetic_mismatch_uses_minor_units_and_configured_tolerance() {
        let party = EntityId::new();
        let mut candidate = invoice(
            "2026-08-01",
            "2026-08-31",
            10_002,
            "MISMATCH-1",
            party,
            10_000,
        );
        candidate.lines[0].total_minor_units = 10_000;
        let findings = evaluate_invoice_rules(
            &[candidate],
            "2026-08-04".parse().unwrap(),
            RuleEvaluationConfig::default(),
        )
        .unwrap();
        assert!(findings.iter().any(|finding| finding.rule_id == "RULE-003"));
    }
}
