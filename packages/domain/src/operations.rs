use serde::{Deserialize, Serialize};

use crate::{DomainError, EntityId, LocalDate, Money, UtcTimestamp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceDirection {
    Payable,
    Receivable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObligationEventKind {
    Settlement,
    WriteOff,
    Reversal,
    DisputeOpened,
    DisputeResolved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObligationState {
    Open,
    PartiallySettled,
    Settled,
    Disputed,
    WrittenOff,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObligationEvent {
    pub id: EntityId,
    pub invoice_id: EntityId,
    pub kind: ObligationEventKind,
    pub amount: Money,
    pub effective_date: LocalDate,
    pub note: String,
    pub source: String,
    pub external_key_sha256: Option<String>,
    pub reverses_event_id: Option<EntityId>,
    pub actor_user_id: EntityId,
    pub occurred_at: UtcTimestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObligationBalance {
    pub invoice_total: Money,
    pub settled: Money,
    pub written_off: Money,
    pub remaining: Money,
    pub state: ObligationState,
}

pub fn reconcile_obligation(
    invoice_total: Money,
    events: &[ObligationEvent],
) -> Result<ObligationBalance, DomainError> {
    let mut settled = Money::new(0, invoice_total.currency);
    let mut written_off = Money::new(0, invoice_total.currency);
    let mut disputed = false;
    for (index, event) in events.iter().enumerate() {
        if event.invoice_id
            != events
                .first()
                .map_or(event.invoice_id, |first| first.invoice_id)
            || event.amount.currency != invoice_total.currency
            || event.amount.minor_units < 0
        {
            return Err(DomainError::InvalidObligationEvent);
        }
        match event.kind {
            ObligationEventKind::Settlement => {
                if event.amount.minor_units == 0 {
                    return Err(DomainError::InvalidObligationEvent);
                }
                settled = settled.checked_add(event.amount)?;
            }
            ObligationEventKind::WriteOff => {
                if event.amount.minor_units == 0 {
                    return Err(DomainError::InvalidObligationEvent);
                }
                written_off = written_off.checked_add(event.amount)?;
            }
            ObligationEventKind::Reversal => {
                let reversed_id = event
                    .reverses_event_id
                    .ok_or(DomainError::InvalidObligationEvent)?;
                if event.amount.minor_units == 0
                    || events[..index].iter().any(|prior| {
                        prior.kind == ObligationEventKind::Reversal
                            && prior.reverses_event_id == Some(reversed_id)
                    })
                {
                    return Err(DomainError::InvalidObligationEvent);
                }
                let original = events[..index]
                    .iter()
                    .find(|candidate| candidate.id == reversed_id)
                    .ok_or(DomainError::InvalidObligationEvent)?;
                if original.amount != event.amount {
                    return Err(DomainError::InvalidObligationEvent);
                }
                match original.kind {
                    ObligationEventKind::Settlement => {
                        settled = settled.checked_sub(event.amount)?;
                    }
                    ObligationEventKind::WriteOff => {
                        written_off = written_off.checked_sub(event.amount)?;
                    }
                    _ => return Err(DomainError::InvalidObligationEvent),
                }
            }
            ObligationEventKind::DisputeOpened => disputed = true,
            ObligationEventKind::DisputeResolved => disputed = false,
        }
    }
    let applied = settled.checked_add(written_off)?;
    if applied.minor_units > invoice_total.minor_units {
        return Err(DomainError::ObligationOverApplied);
    }
    let remaining = invoice_total.checked_sub(applied)?;
    let state = if disputed {
        ObligationState::Disputed
    } else if remaining.minor_units == 0 && written_off.minor_units > 0 {
        ObligationState::WrittenOff
    } else if remaining.minor_units == 0 {
        ObligationState::Settled
    } else if settled.minor_units > 0 || written_off.minor_units > 0 {
        ObligationState::PartiallySettled
    } else {
        ObligationState::Open
    };
    Ok(ObligationBalance {
        invoice_total,
        settled,
        written_off,
        remaining,
        state,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CashSnapshot {
    pub id: EntityId,
    pub snapshot_date: LocalDate,
    pub available: Money,
    pub source: String,
    pub note: String,
    pub actor_user_id: EntityId,
    pub created_at: UtcTimestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CashBucketKind {
    Inflow,
    Outflow,
    Reserved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CashScenarioBucket {
    pub id: EntityId,
    pub snapshot_id: EntityId,
    pub name: String,
    pub kind: CashBucketKind,
    pub amount: Money,
    pub due_date: Option<LocalDate>,
    pub actor_user_id: EntityId,
    pub created_at: UtcTimestamp,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CurrencyCode;

    fn event(kind: ObligationEventKind, amount: i64) -> ObligationEvent {
        ObligationEvent {
            id: EntityId::new(),
            invoice_id: EntityId::from_uuid(uuid::Uuid::from_u128(1)),
            kind,
            amount: Money::new(amount, CurrencyCode::parse("INR").unwrap()),
            effective_date: "2026-08-07".parse().unwrap(),
            note: "Golden fixture".to_owned(),
            source: "manual".to_owned(),
            external_key_sha256: None,
            reverses_event_id: None,
            actor_user_id: EntityId::new(),
            occurred_at: UtcTimestamp::from_unix_milliseconds(1),
        }
    }

    #[test]
    fn golden_obligation_reconciles_exact_minor_units() {
        let currency = CurrencyCode::parse("INR").unwrap();
        let mut settlement = event(ObligationEventKind::Settlement, 250_000);
        let write_off = event(ObligationEventKind::WriteOff, 50_000);
        settlement.invoice_id = write_off.invoice_id;
        let balance =
            reconcile_obligation(Money::new(1_000_000, currency), &[settlement, write_off])
                .unwrap();
        assert_eq!(balance.settled.minor_units, 250_000);
        assert_eq!(balance.written_off.minor_units, 50_000);
        assert_eq!(balance.remaining.minor_units, 700_000);
        assert_eq!(balance.state, ObligationState::PartiallySettled);
    }

    #[test]
    fn approval_without_events_is_open_not_paid() {
        let balance =
            reconcile_obligation(Money::new(1_00, CurrencyCode::parse("INR").unwrap()), &[])
                .unwrap();
        assert_eq!(balance.state, ObligationState::Open);
        assert_eq!(balance.remaining.minor_units, 1_00);
    }

    #[test]
    fn reversal_must_be_exact_once_and_follow_original() {
        let currency = CurrencyCode::parse("INR").unwrap();
        let settlement = event(ObligationEventKind::Settlement, 25_000);
        let mut reversal = event(ObligationEventKind::Reversal, 25_000);
        reversal.invoice_id = settlement.invoice_id;
        reversal.reverses_event_id = Some(settlement.id);
        let balance = reconcile_obligation(
            Money::new(100_000, currency),
            &[settlement.clone(), reversal.clone()],
        )
        .unwrap();
        assert_eq!(balance.remaining.minor_units, 100_000);

        let mut duplicate = reversal.clone();
        duplicate.id = EntityId::new();
        assert!(reconcile_obligation(
            Money::new(100_000, currency),
            &[settlement.clone(), reversal, duplicate],
        )
        .is_err());

        let mut partial = event(ObligationEventKind::Reversal, 10_000);
        partial.invoice_id = settlement.invoice_id;
        partial.reverses_event_id = Some(settlement.id);
        assert!(
            reconcile_obligation(Money::new(100_000, currency), &[settlement, partial]).is_err()
        );
    }
}
