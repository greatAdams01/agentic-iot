use std::collections::BTreeMap;
use std::time::Duration;

use dcra::assurance::{ActionContract, evaluate_all};
use dcra::clock::ControlledClock;
use dcra::evidence::{EvidenceRecord, EvidenceStatus};

fn main() {
    let mut clock = ControlledClock::default();
    let evidence = BTreeMap::from([(
        "room_clear".into(),
        EvidenceRecord {
            status: EvidenceStatus::Valid,
            expires_at: Duration::from_secs(5),
        },
    )]);
    let contracts = [ActionContract {
        action_id: "close_door".into(),
        required_evidence: vec!["room_clear".into()],
    }];

    for _ in 0..2 {
        println!(
            "t={:?}: {:?}",
            clock.now(),
            evaluate_all(&contracts, &evidence, clock.now())
        );
        clock.advance(Duration::from_secs(5));
    }
}
