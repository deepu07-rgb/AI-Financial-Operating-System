use serde::{Deserialize, Serialize};

use crate::{CurrencyCode, DomainError, EntityId, LocalDate, Money, ObligationState};

pub const PAYMENT_PRIORITY_RULE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentPriorityInput {
    pub invoice_id: EntityId,
    pub party_id: Option<EntityId>,
    pub invoice_number: Option<String>,
    pub party_name: Option<String>,
    pub due_date: Option<LocalDate>,
    pub remaining: Money,
    pub settlement_state: ObligationState,
    pub supplier_criticality: Option<u8>,
    pub payment_terms_days: Option<u32>,
    pub penalty_basis_points: Option<u32>,
    pub penalty_terms_evidence: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriorityComponent {
    pub key: String,
    pub points: i64,
    pub explanation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaymentPriorityDecision {
    pub rank: u32,
    pub invoice_id: EntityId,
    pub party_id: Option<EntityId>,
    pub invoice_number: Option<String>,
    pub party_name: Option<String>,
    pub due_date: Option<LocalDate>,
    pub remaining: Money,
    pub settlement_state: ObligationState,
    pub score: i64,
    pub supplier_criticality: Option<u8>,
    pub payment_terms_days: Option<u32>,
    pub penalty_basis_points: Option<u32>,
    pub estimated_penalty: Option<Money>,
    pub penalty_terms_evidence: Option<String>,
    pub components: Vec<PriorityComponent>,
    pub missing_inputs: Vec<String>,
    pub rule_version: u32,
}

pub fn rank_payment_priorities(
    inputs: &[PaymentPriorityInput],
    as_of_date: LocalDate,
) -> Result<Vec<PaymentPriorityDecision>, DomainError> {
    let mut decisions = inputs
        .iter()
        .filter(|input| {
            input.remaining.minor_units > 0 && input.settlement_state != ObligationState::Disputed
        })
        .map(|input| priority_decision(input, as_of_date))
        .collect::<Result<Vec<_>, _>>()?;
    decisions.sort_by(|left, right| {
        left.remaining
            .currency
            .to_string()
            .cmp(&right.remaining.currency.to_string())
            .then_with(|| right.score.cmp(&left.score))
            .then_with(|| left.due_date.cmp(&right.due_date))
            .then_with(|| {
                left.invoice_id
                    .to_string()
                    .cmp(&right.invoice_id.to_string())
            })
    });
    let mut rank = 0_u32;
    let mut prior_currency: Option<CurrencyCode> = None;
    for decision in &mut decisions {
        if prior_currency != Some(decision.remaining.currency) {
            rank = 0;
            prior_currency = Some(decision.remaining.currency);
        }
        rank = rank.saturating_add(1);
        decision.rank = rank;
    }
    Ok(decisions)
}

fn priority_decision(
    input: &PaymentPriorityInput,
    as_of_date: LocalDate,
) -> Result<PaymentPriorityDecision, DomainError> {
    let mut components = Vec::new();
    let mut missing_inputs = Vec::new();
    let due_points = match input.due_date {
        Some(due_date) => {
            let days_until_due = as_of_date.days_until(due_date);
            let (points, explanation) = if days_until_due < 0 {
                let overdue_days = days_until_due.saturating_abs().min(90);
                (
                    1_200_i64.saturating_add(overdue_days.saturating_mul(30)),
                    format!("{overdue_days} day(s) overdue."),
                )
            } else if days_until_due <= 30 {
                (
                    (30_i64.saturating_sub(days_until_due)).saturating_mul(40),
                    format!("Due in {days_until_due} day(s)."),
                )
            } else {
                (
                    0,
                    format!("Due in {days_until_due} day(s); outside 30-day window."),
                )
            };
            components.push(PriorityComponent {
                key: "due_urgency".to_owned(),
                points,
                explanation,
            });
            points
        }
        None => {
            missing_inputs.push("Confirmed due date".to_owned());
            0
        }
    };
    let criticality_points = match input.supplier_criticality {
        Some(value @ 1..=5) => {
            let points = i64::from(value).saturating_mul(600);
            components.push(PriorityComponent {
                key: "supplier_criticality".to_owned(),
                points,
                explanation: format!("Supplier criticality {value}/5."),
            });
            points
        }
        _ => {
            missing_inputs.push("Supplier criticality (1-5)".to_owned());
            0
        }
    };
    if input.payment_terms_days.is_none() {
        missing_inputs.push("Confirmed payment terms".to_owned());
    }
    let overdue_days = input
        .due_date
        .map(|due| due.days_until(as_of_date).max(0))
        .unwrap_or(0);
    let (penalty_points, estimated_penalty) = match input.penalty_basis_points {
        Some(basis_points) if overdue_days > 0 => {
            let estimate = i128::from(input.remaining.minor_units)
                .checked_mul(i128::from(basis_points))
                .and_then(|value| value.checked_mul(i128::from(overdue_days)))
                .and_then(|value| value.checked_div(10_000 * 365))
                .and_then(|value| i64::try_from(value).ok())
                .ok_or(DomainError::MoneyOverflow)?;
            let points = i64::from(basis_points.min(2_500));
            components.push(PriorityComponent {
                key: "penalty_exposure".to_owned(),
                points,
                explanation: format!(
                    "Confirmed annual penalty rate {basis_points} bps; estimate uses {overdue_days} overdue day(s)."
                ),
            });
            (points, Some(Money::new(estimate, input.remaining.currency)))
        }
        Some(basis_points) => {
            components.push(PriorityComponent {
                key: "penalty_exposure".to_owned(),
                points: 0,
                explanation: format!(
                    "Confirmed annual penalty rate {basis_points} bps; not overdue."
                ),
            });
            (0, Some(Money::new(0, input.remaining.currency)))
        }
        None => {
            missing_inputs.push("Confirmed penalty terms".to_owned());
            (0, None)
        }
    };
    let partial_points = if input.settlement_state == ObligationState::PartiallySettled {
        components.push(PriorityComponent {
            key: "partial_settlement".to_owned(),
            points: 300,
            explanation: "Partially settled obligation has a confirmed remaining balance."
                .to_owned(),
        });
        300
    } else {
        0
    };
    Ok(PaymentPriorityDecision {
        rank: 0,
        invoice_id: input.invoice_id,
        party_id: input.party_id,
        invoice_number: input.invoice_number.clone(),
        party_name: input.party_name.clone(),
        due_date: input.due_date,
        remaining: input.remaining,
        settlement_state: input.settlement_state,
        score: due_points
            .saturating_add(criticality_points)
            .saturating_add(penalty_points)
            .saturating_add(partial_points),
        supplier_criticality: input.supplier_criticality,
        payment_terms_days: input.payment_terms_days,
        penalty_basis_points: input.penalty_basis_points,
        estimated_penalty,
        penalty_terms_evidence: input.penalty_terms_evidence.clone(),
        components,
        missing_inputs,
        rule_version: PAYMENT_PRIORITY_RULE_VERSION,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(currency: &str, due: &str, criticality: Option<u8>) -> PaymentPriorityInput {
        PaymentPriorityInput {
            invoice_id: EntityId::new(),
            party_id: Some(EntityId::new()),
            invoice_number: Some("INV-1".to_owned()),
            party_name: Some("Supplier".to_owned()),
            due_date: Some(due.parse().unwrap()),
            remaining: Money::new(100_000, CurrencyCode::parse(currency).unwrap()),
            settlement_state: ObligationState::Open,
            supplier_criticality: criticality,
            payment_terms_days: Some(30),
            penalty_basis_points: Some(1_200),
            penalty_terms_evidence: Some("Reviewed contract".to_owned()),
        }
    }

    #[test]
    fn priority_is_explainable_and_ranked_only_within_currency() {
        let as_of = "2026-08-07".parse().unwrap();
        let urgent = input("INR", "2026-08-01", Some(5));
        let later = input("INR", "2026-08-20", Some(1));
        let usd = input("USD", "2026-08-01", Some(5));
        let decisions = rank_payment_priorities(&[later, usd, urgent.clone()], as_of).unwrap();
        let inr = decisions
            .iter()
            .filter(|decision| decision.remaining.currency.to_string() == "INR")
            .collect::<Vec<_>>();
        assert_eq!(inr[0].invoice_id, urgent.invoice_id);
        assert_eq!(inr[0].rank, 1);
        assert!(inr[0].estimated_penalty.unwrap().minor_units > 0);
        assert_eq!(
            decisions
                .iter()
                .find(|item| item.remaining.currency.to_string() == "USD")
                .unwrap()
                .rank,
            1
        );
    }

    #[test]
    fn disputed_obligations_are_held_and_missing_inputs_are_named() {
        let as_of = "2026-08-07".parse().unwrap();
        let mut disputed = input("INR", "2026-08-01", Some(5));
        disputed.settlement_state = ObligationState::Disputed;
        let mut incomplete = input("INR", "2026-08-01", None);
        incomplete.payment_terms_days = None;
        incomplete.penalty_basis_points = None;
        let decisions = rank_payment_priorities(&[disputed, incomplete], as_of).unwrap();
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].missing_inputs.len(), 3);
    }
}
