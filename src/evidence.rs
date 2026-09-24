use std::time::Duration;

/// Support for a predicate, rather than whether its physical situation is desirable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssuranceStatus {
    Valid,
    Unknown,
    Invalid,
}

/// A VALID value always has a horizon; unsupported values cannot carry one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssuranceValue {
    Valid { horizon: Duration },
    Unknown,
    Invalid,
}

impl AssuranceValue {
    pub fn status(self) -> AssuranceStatus {
        match self {
            Self::Valid { .. } => AssuranceStatus::Valid,
            Self::Unknown => AssuranceStatus::Unknown,
            Self::Invalid => AssuranceStatus::Invalid,
        }
    }
    pub fn horizon(self) -> Option<Duration> {
        match self {
            Self::Valid { horizon } => Some(horizon),
            _ => None,
        }
    }
}

/// Latest observation metadata and its current logical status.
/// Runtime expiry changes status, but preserves the observation's version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceAtom {
    pub evidence_id: String,
    pub evidence_type: String,
    pub predicate: String,
    pub source_id: String,
    pub version: u64,
    pub observed_at: Duration,
    pub expires_at: Duration,
    pub status: AssuranceStatus,
    pub payload_hash: Option<String>,
}

impl EvidenceAtom {
    pub fn value_at(&self, now: Duration) -> AssuranceValue {
        if self.observed_at > now {
            return AssuranceValue::Unknown;
        }
        match self.status {
            AssuranceStatus::Valid if now < self.expires_at => AssuranceValue::Valid {
                horizon: self.expires_at,
            },
            AssuranceStatus::Invalid => AssuranceValue::Invalid,
            _ => AssuranceValue::Unknown,
        }
    }
}
