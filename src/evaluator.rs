//! Readable full reference evaluation. No cached or incremental decisions.
use crate::evidence::{AssuranceValue, EvidenceAtom};
use crate::graph::{AssuranceGraph, NodeKind};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Witness {
    pub node_id: String,
    pub evidence_ids: BTreeSet<String>,
    pub horizon: Duration,
    /// None for atomic evidence; otherwise identifies the selected rule.
    pub justification_id: Option<String>,
    pub premise_ids: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Evaluation {
    pub assurance_by_node: BTreeMap<String, AssuranceValue>,
    pub assurance_by_justification: BTreeMap<String, AssuranceValue>,
    pub preferred_witness_by_node: BTreeMap<String, Witness>,
}

/// Missing observations are UNKNOWN. Inputs are keyed by evidence_id; mismatched
/// identities fail closed. Runtime validates metadata before calling this function.
/// A threshold is INVALID only when even all UNKNOWN premises could not satisfy it.
pub fn evaluate_full(
    graph: &AssuranceGraph,
    evidence: &BTreeMap<String, EvidenceAtom>,
    now: Duration,
) -> Evaluation {
    let mut result = Evaluation::default();
    for id in graph.topological_order() {
        if graph.nodes()[id] == NodeKind::Evidence {
            let value = evidence
                .get(id)
                .filter(|atom| atom.evidence_id == *id)
                .map_or(AssuranceValue::Unknown, |atom| atom.value_at(now));
            result.assurance_by_node.insert(id.clone(), value);
            if let AssuranceValue::Valid { horizon } = value {
                result.preferred_witness_by_node.insert(
                    id.clone(),
                    Witness {
                        node_id: id.clone(),
                        evidence_ids: BTreeSet::from([id.clone()]),
                        horizon,
                        justification_id: None,
                        premise_ids: Vec::new(),
                    },
                );
            }
            continue;
        }
        let mut candidates = Vec::new();
        let mut any_unknown = false;
        for rule_id in &graph.justifications_by_conclusion()[id] {
            let rule = &graph.justifications()[rule_id];
            let mut supports: Vec<&Witness> = rule
                .premises
                .iter()
                .filter_map(|p| result.preferred_witness_by_node.get(p))
                .collect();
            let unknown = rule
                .premises
                .iter()
                .filter(|p| result.assurance_by_node[*p] == AssuranceValue::Unknown)
                .count();
            let value = if supports.len() >= rule.threshold {
                supports.sort_by(|a, b| {
                    b.horizon
                        .cmp(&a.horizon)
                        .then(a.evidence_ids.len().cmp(&b.evidence_ids.len()))
                        .then(a.node_id.cmp(&b.node_id))
                });
                supports.truncate(rule.threshold);
                let horizon = supports.iter().map(|w| w.horizon).min().unwrap();
                candidates.push(Witness {
                    node_id: id.clone(),
                    evidence_ids: supports
                        .iter()
                        .flat_map(|w| w.evidence_ids.iter().cloned())
                        .collect(),
                    horizon,
                    justification_id: Some(rule_id.clone()),
                    premise_ids: supports.iter().map(|w| w.node_id.clone()).collect(),
                });
                AssuranceValue::Valid { horizon }
            } else if supports.len() + unknown >= rule.threshold {
                any_unknown = true;
                AssuranceValue::Unknown
            } else {
                AssuranceValue::Invalid
            };
            result
                .assurance_by_justification
                .insert(rule_id.clone(), value);
        }
        candidates.sort_by(|a, b| {
            b.horizon
                .cmp(&a.horizon)
                .then(a.evidence_ids.len().cmp(&b.evidence_ids.len()))
                .then(a.justification_id.cmp(&b.justification_id))
        });
        let value = if let Some(witness) = candidates.into_iter().next() {
            let value = AssuranceValue::Valid {
                horizon: witness.horizon,
            };
            result.preferred_witness_by_node.insert(id.clone(), witness);
            value
        } else if any_unknown {
            AssuranceValue::Unknown
        } else {
            AssuranceValue::Invalid
        };
        result.assurance_by_node.insert(id.clone(), value);
    }
    result
}
