//! Run with: cargo run --offline --example week_one
use dcra::evidence::{AssuranceStatus as Status, EvidenceAtom};
use dcra::graph::{AssuranceGraph, Justification, Node, NodeKind};
use dcra::runtime::{AssuranceRuntime, AuditEvent};
use std::time::Duration;

fn observation(id: &str, status: Status, version: u64, expiry: u64) -> EvidenceAtom {
    EvidenceAtom {
        evidence_id: id.into(),
        evidence_type: "sensor".into(),
        predicate: "gas_high".into(),
        source_id: id.into(),
        version,
        observed_at: Duration::ZERO,
        expires_at: Duration::from_secs(expiry),
        status,
        payload_hash: None,
    }
}
fn explain(runtime: &AssuranceRuntime, id: &str) {
    println!(
        "t={:?}, epoch={}: {id} = {:?}",
        runtime.now(),
        runtime.epoch(),
        runtime.evaluation().assurance_by_node[id]
    );
    if let Some(witness) = runtime.evaluation().preferred_witness_by_node.get(id) {
        println!(
            "  supporting evidence: {:?}; supported until {:?}; rule: {:?}",
            witness.evidence_ids, witness.horizon, witness.justification_id
        );
    } else {
        println!("  no valid supporting witness");
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("C1: 2-of-3 hazard confirmation survives one sensor loss");
    let graph = AssuranceGraph::new(
        vec![
            Node::new("s1", NodeKind::Evidence),
            Node::new("s2", NodeKind::Evidence),
            Node::new("thermal", NodeKind::Evidence),
            Node::new("hazard", NodeKind::Derived),
        ],
        vec![Justification {
            justification_id: "hazard-quorum".into(),
            premises: vec!["s1".into(), "s2".into(), "thermal".into()],
            threshold: 2,
            conclusion: "hazard".into(),
        }],
    )?;
    let mut runtime = AssuranceRuntime::new(graph);
    for id in ["s1", "s2", "thermal"] {
        runtime.update(observation(id, Status::Valid, 1, 10))?;
    }
    explain(&runtime, "hazard");
    runtime.update(observation("s2", Status::Unknown, 2, 10))?;
    explain(&runtime, "hazard");

    println!("\nC2: version 10 expires at t=2 with no new sensor message");
    let graph = AssuranceGraph::new(vec![Node::new("sensor", NodeKind::Evidence)], vec![])?;
    let mut runtime = AssuranceRuntime::new(graph);
    runtime.update(observation("sensor", Status::Valid, 10, 2))?;
    explain(&runtime, "sensor");
    runtime.advance_to(Duration::from_secs(3))?;
    explain(&runtime, "sensor");
    println!(
        "  sensor version remains {}",
        runtime.evidence()["sensor"].version
    );
    for entry in runtime.audit() {
        if matches!(entry.event, AuditEvent::EvidenceExpired { .. }) {
            println!(
                "  audit: t={:?}, epoch={}, {:?}",
                entry.at, entry.epoch, entry.event
            );
        }
    }
    Ok(())
}
