use std::collections::BTreeMap;
use std::time::Duration;

use dcra::assurance::{ActionContract, BlockReason, Permission, evaluate_all};
use dcra::clock::ControlledClock;
use dcra::evidence::{EvidenceRecord, EvidenceStatus};

fn contract(action: &str, requirements: &[&str]) -> ActionContract {
    ActionContract {
        action_id: action.into(),
        required_evidence: requirements.iter().map(|id| (*id).into()).collect(),
    }
}

fn record(status: EvidenceStatus) -> EvidenceRecord {
    EvidenceRecord {
        status,
        expires_at: Duration::from_secs(5),
    }
}

#[test]
fn valid_unexpired_evidence_allows_action() {
    let clock = ControlledClock::default();
    let evidence = BTreeMap::from([("room_clear".into(), record(EvidenceStatus::Valid))]);
    let decisions = evaluate_all(
        &[contract("close_door", &["room_clear"])],
        &evidence,
        clock.now(),
    );
    assert_eq!(decisions[0].action_id, "close_door");
    assert_eq!(decisions[0].permission, Permission::Allowed);
}

#[test]
fn expiry_blocks_at_deadline_and_after_without_a_new_observation() {
    let mut clock = ControlledClock::default();
    let evidence = BTreeMap::from([("room_clear".into(), record(EvidenceStatus::Valid))]);
    let contracts = [contract("close_door", &["room_clear"])];
    clock.advance(Duration::from_secs(4));
    assert_eq!(
        evaluate_all(&contracts, &evidence, clock.now())[0].permission,
        Permission::Allowed
    );
    for _ in 0..2 {
        clock.advance(Duration::from_secs(1));
        assert_eq!(
            evaluate_all(&contracts, &evidence, clock.now())[0].permission,
            Permission::Blocked(vec![BlockReason::UnknownEvidence("room_clear".into())])
        );
    }
    assert_eq!(evidence["room_clear"].status, EvidenceStatus::Valid);
}

#[test]
fn unknown_evidence_blocks_action() {
    let evidence = BTreeMap::from([("room_clear".into(), record(EvidenceStatus::Unknown))]);
    assert_eq!(
        evaluate_all(
            &[contract("close_door", &["room_clear"])],
            &evidence,
            Duration::ZERO
        )[0]
        .permission,
        Permission::Blocked(vec![BlockReason::UnknownEvidence("room_clear".into())])
    );
}

#[test]
fn invalid_evidence_blocks_action() {
    let evidence = BTreeMap::from([("room_clear".into(), record(EvidenceStatus::Invalid))]);
    assert_eq!(
        evaluate_all(
            &[contract("close_door", &["room_clear"])],
            &evidence,
            Duration::ZERO
        )[0]
        .permission,
        Permission::Blocked(vec![BlockReason::InvalidEvidence("room_clear".into())])
    );
}

#[test]
fn missing_evidence_and_empty_contracts_block() {
    let decisions = evaluate_all(
        &[
            contract("close_door", &["room_clear"]),
            contract("unconfigured", &[]),
        ],
        &BTreeMap::new(),
        Duration::ZERO,
    );
    assert_eq!(
        decisions[0].permission,
        Permission::Blocked(vec![BlockReason::MissingEvidence("room_clear".into())])
    );
    assert_eq!(
        decisions[1].permission,
        Permission::Blocked(vec![BlockReason::NoRequirements])
    );
}

#[test]
fn all_requirements_are_needed_but_unrelated_failure_does_not_block() {
    let mut evidence = BTreeMap::from([
        ("hazard_confirmed".into(), record(EvidenceStatus::Valid)),
        ("fan_healthy".into(), record(EvidenceStatus::Unknown)),
    ]);
    let contracts = [
        contract("start_fan", &["hazard_confirmed", "fan_healthy"]),
        contract("activate_alarm", &["hazard_confirmed"]),
    ];
    let decisions = evaluate_all(&contracts, &evidence, Duration::ZERO);
    assert_eq!(
        decisions[0].permission,
        Permission::Blocked(vec![BlockReason::UnknownEvidence("fan_healthy".into())])
    );
    assert_eq!(decisions[1].permission, Permission::Allowed);
    evidence.insert("fan_healthy".into(), record(EvidenceStatus::Valid));
    assert_eq!(
        evaluate_all(&contracts, &evidence, Duration::ZERO)[0].permission,
        Permission::Allowed
    );
}
