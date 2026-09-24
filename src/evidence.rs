use std::time::Duration;

/// Support for a predicate, rather than whether its physical situation is desirable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceStatus {
    Valid,
    Unknown,
    Invalid,
}

/// One observation's declared status and validity deadline on the simulation clock.
/// Evidence identity is its key in the evaluator's evidence map.
#[derive(Debug, Clone, Copy)]
pub struct EvidenceRecord {
    pub status: EvidenceStatus,
    pub expires_at: Duration,
}

impl EvidenceRecord {
    /// Expired VALID evidence becomes effectively UNKNOWN, including at the deadline.
    /// UNKNOWN and INVALID remain unsupported until replaced by a new record.
    /// Evaluation does not mutate the stored observation.
    pub fn status_at(&self, now: Duration) -> EvidenceStatus {
        match self.status {
            EvidenceStatus::Valid if now >= self.expires_at => EvidenceStatus::Unknown,
            status => status,
        }
    }
}
