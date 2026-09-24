//! Single-threaded deterministic evidence lifecycle with explicit audit events.
use crate::clock::ControlledClock;
use crate::evaluator::{Evaluation, Witness, evaluate_full};
use crate::evidence::{AssuranceStatus, AssuranceValue, EvidenceAtom};
use crate::graph::{AssuranceGraph, NodeKind};
use std::collections::BTreeMap;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    NotEvidenceNode(String),
    EmptyMetadata,
    InvalidObservationTime,
    NonIncreasingVersion(String),
    ChangedIdentity(String),
    TimeWentBackwards,
    TimeOverflow,
    EpochOverflow,
}
impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "evidence update rejected: {self:?}")
    }
}
impl std::error::Error for RuntimeError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditEvent {
    EvidenceUpdated {
        previous: Option<EvidenceAtom>,
        current: EvidenceAtom,
    },
    /// Explicit VALID -> UNKNOWN transition; no new observation version.
    EvidenceExpired { evidence_id: String, version: u64 },
    AssuranceChanged {
        node_id: String,
        previous: AssuranceValue,
        current: AssuranceValue,
    },
    WitnessChanged {
        node_id: String,
        previous: Option<Witness>,
        current: Option<Witness>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEntry {
    pub at: Duration,
    pub epoch: u64,
    pub event: AuditEvent,
}

#[derive(Debug)]
pub struct AssuranceRuntime {
    graph: AssuranceGraph,
    clock: ControlledClock,
    evidence: BTreeMap<String, EvidenceAtom>,
    epoch: u64,
    evaluation: Evaluation,
    audit: Vec<AuditEntry>,
}

impl AssuranceRuntime {
    pub fn new(graph: AssuranceGraph) -> Self {
        let evidence = BTreeMap::new();
        let evaluation = evaluate_full(&graph, &evidence, Duration::ZERO);
        Self {
            graph,
            clock: ControlledClock::default(),
            evidence,
            epoch: 0,
            evaluation,
            audit: Vec::new(),
        }
    }
    pub fn now(&self) -> Duration {
        self.clock.now()
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    pub fn graph(&self) -> &AssuranceGraph {
        &self.graph
    }
    pub fn evidence(&self) -> &BTreeMap<String, EvidenceAtom> {
        &self.evidence
    }
    pub fn evaluation(&self) -> &Evaluation {
        &self.evaluation
    }
    pub fn audit(&self) -> &[AuditEntry] {
        &self.audit
    }

    /// Updates are atomic on validation failure. Versions must strictly increase
    /// per evidence ID; the ID's source, type and predicate remain stable.
    pub fn update(&mut self, atom: EvidenceAtom) -> Result<(), RuntimeError> {
        if self.graph.nodes().get(&atom.evidence_id) != Some(&NodeKind::Evidence) {
            return Err(RuntimeError::NotEvidenceNode(atom.evidence_id));
        }
        if [&atom.evidence_type, &atom.predicate, &atom.source_id]
            .iter()
            .any(|s| s.trim().is_empty())
        {
            return Err(RuntimeError::EmptyMetadata);
        }
        if atom.observed_at > self.now() || atom.expires_at < atom.observed_at {
            return Err(RuntimeError::InvalidObservationTime);
        }
        if let Some(previous) = self.evidence.get(&atom.evidence_id) {
            if atom.version <= previous.version {
                return Err(RuntimeError::NonIncreasingVersion(atom.evidence_id));
            }
            if atom.source_id != previous.source_id
                || atom.predicate != previous.predicate
                || atom.evidence_type != previous.evidence_type
            {
                return Err(RuntimeError::ChangedIdentity(atom.evidence_id));
            }
        }
        let already_expired =
            atom.status == AssuranceStatus::Valid && atom.expires_at <= self.now();
        self.epoch
            .checked_add(if already_expired { 2 } else { 1 })
            .ok_or(RuntimeError::EpochOverflow)?;
        self.epoch += 1;
        let previous = self.evidence.insert(atom.evidence_id.clone(), atom.clone());
        self.record(AuditEvent::EvidenceUpdated {
            previous,
            current: atom.clone(),
        });
        if already_expired {
            self.expire(&atom.evidence_id);
        }
        self.reevaluate();
        Ok(())
    }

    pub fn advance(&mut self, elapsed: Duration) -> Result<(), RuntimeError> {
        let target = self
            .now()
            .checked_add(elapsed)
            .ok_or(RuntimeError::TimeOverflow)?;
        self.advance_to(target)
    }

    /// Process each deadline in chronological order, then evidence-ID order.
    /// Deadlines are selected from current observations: replacement cannot leave
    /// a stale timer that later expires the replacement observation.
    pub fn advance_to(&mut self, target: Duration) -> Result<(), RuntimeError> {
        if target < self.now() {
            return Err(RuntimeError::TimeWentBackwards);
        }
        let mut due: Vec<_> = self
            .evidence
            .values()
            .filter(|atom| atom.status == AssuranceStatus::Valid && atom.expires_at <= target)
            .map(|atom| (atom.expires_at, atom.evidence_id.clone()))
            .collect();
        due.sort();
        self.epoch
            .checked_add(due.len() as u64)
            .ok_or(RuntimeError::EpochOverflow)?;
        let mut events = due.into_iter().peekable();
        while let Some((deadline, id)) = events.next() {
            self.clock.advance(deadline.saturating_sub(self.now()));
            self.expire(&id);
            while events.peek().is_some_and(|(at, _)| *at == deadline) {
                let (_, id) = events.next().unwrap();
                self.expire(&id);
            }
            // All evidence expiring at this instant transitions before evaluation.
            self.reevaluate();
        }
        self.clock.advance(target - self.now());
        Ok(())
    }

    fn expire(&mut self, id: &str) {
        self.epoch += 1;
        let atom = self.evidence.get_mut(id).unwrap();
        atom.status = AssuranceStatus::Unknown;
        let version = atom.version;
        self.record(AuditEvent::EvidenceExpired {
            evidence_id: id.into(),
            version,
        });
    }
    fn record(&mut self, event: AuditEvent) {
        self.audit.push(AuditEntry {
            at: self.now(),
            epoch: self.epoch,
            event,
        });
    }
    fn reevaluate(&mut self) {
        let next = evaluate_full(&self.graph, &self.evidence, self.now());
        // Ordered keys make the event log reproducible independent of input order.
        for (id, current) in &next.assurance_by_node {
            let previous = self.evaluation.assurance_by_node[id];
            if previous != *current {
                self.record(AuditEvent::AssuranceChanged {
                    node_id: id.clone(),
                    previous,
                    current: *current,
                });
            }
            let previous = self.evaluation.preferred_witness_by_node.get(id).cloned();
            let current = next.preferred_witness_by_node.get(id).cloned();
            if previous != current {
                self.record(AuditEvent::WitnessChanged {
                    node_id: id.clone(),
                    previous,
                    current,
                });
            }
        }
        self.evaluation = next;
    }
}
