use std::collections::BTreeMap;
use std::time::Duration;

use crate::evidence::{EvidenceRecord, EvidenceStatus};

/// A simple AND rule: every listed evidence item must currently be VALID.
#[derive(Debug)]
pub struct ActionContract {
    pub action_id: String,
    pub required_evidence: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BlockReason {
    /// Empty contracts are rejected to prevent accidental unconditional permission.
    NoRequirements,
    MissingEvidence(String),
    UnknownEvidence(String),
    InvalidEvidence(String),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Permission {
    Allowed,
    Blocked(Vec<BlockReason>),
}

#[derive(Debug, PartialEq, Eq)]
pub struct ActionDecision {
    pub action_id: String,
    pub permission: Permission,
}

/// Recomputes every contract from scratch at one snapshot of virtual time.
///
/// Results retain input contract order; reasons retain requirement order.
/// There is no cached permission, dispatch, or background expiry processing.
pub fn evaluate_all(
    contracts: &[ActionContract],
    evidence: &BTreeMap<String, EvidenceRecord>,
    now: Duration,
) -> Vec<ActionDecision> {
    contracts
        .iter()
        .map(|contract| {
            let mut reasons = Vec::new();
            if contract.required_evidence.is_empty() {
                reasons.push(BlockReason::NoRequirements);
            }
            for id in &contract.required_evidence {
                match evidence.get(id).map(|record| record.status_at(now)) {
                    Some(EvidenceStatus::Valid) => {}
                    Some(EvidenceStatus::Unknown) => {
                        reasons.push(BlockReason::UnknownEvidence(id.clone()));
                    }
                    Some(EvidenceStatus::Invalid) => {
                        reasons.push(BlockReason::InvalidEvidence(id.clone()));
                    }
                    None => reasons.push(BlockReason::MissingEvidence(id.clone())),
                }
            }
            ActionDecision {
                action_id: contract.action_id.clone(),
                permission: if reasons.is_empty() {
                    Permission::Allowed
                } else {
                    Permission::Blocked(reasons)
                },
            }
        })
        .collect()
}
