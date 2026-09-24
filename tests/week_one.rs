use dcra::evaluator::evaluate_full;
use dcra::evidence::{AssuranceStatus as Status, AssuranceValue as Value, EvidenceAtom};
use dcra::graph::{AssuranceGraph, GraphError, Justification, Node, NodeKind};
use dcra::runtime::{AssuranceRuntime, AuditEvent, RuntimeError};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

fn t(seconds: u64) -> Duration {
    Duration::from_secs(seconds)
}
fn atom(id: &str, status: Status, expiry: u64) -> EvidenceAtom {
    EvidenceAtom {
        evidence_id: id.into(),
        evidence_type: "sensor".into(),
        predicate: format!("{id}.supports"),
        source_id: id.into(),
        version: 1,
        observed_at: t(0),
        expires_at: t(expiry),
        status,
        payload_hash: None,
    }
}
fn rule(id: &str, premises: &[&str], threshold: usize, conclusion: &str) -> Justification {
    Justification {
        justification_id: id.into(),
        premises: premises.iter().map(|p| (*p).into()).collect(),
        threshold,
        conclusion: conclusion.into(),
    }
}
fn graph(evidence: &[&str], derived: &[&str], rules: Vec<Justification>) -> AssuranceGraph {
    AssuranceGraph::new(
        evidence
            .iter()
            .map(|id| Node::new(*id, NodeKind::Evidence))
            .chain(derived.iter().map(|id| Node::new(*id, NodeKind::Derived)))
            .collect(),
        rules,
    )
    .unwrap()
}
fn quorum() -> AssuranceGraph {
    graph(
        &["s1", "s2", "thermal"],
        &["hazard"],
        vec![rule("quorum", &["s1", "s2", "thermal"], 2, "hazard")],
    )
}
fn ids(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|v| (*v).into()).collect()
}

#[test]
fn c1_quorum_survives_one_sensor_loss() {
    let mut runtime = AssuranceRuntime::new(quorum());
    for id in ["s1", "s2", "thermal"] {
        runtime.update(atom(id, Status::Valid, 10)).unwrap();
    }
    let mut lost = atom("s2", Status::Unknown, 10);
    lost.version = 2;
    runtime.update(lost).unwrap();
    assert_eq!(
        runtime.evaluation().assurance_by_node["hazard"],
        Value::Valid { horizon: t(10) }
    );
    assert_eq!(
        runtime.evaluation().preferred_witness_by_node["hazard"].evidence_ids,
        ids(&["s1", "thermal"])
    );
    // The all-sensors-required policy loses support on the same observations.
    let all_required = graph(
        &["s1", "s2", "thermal"],
        &["hazard"],
        vec![rule("all-required", &["s1", "s2", "thermal"], 3, "hazard")],
    );
    assert_eq!(
        evaluate_full(&all_required, runtime.evidence(), runtime.now()).assurance_by_node["hazard"],
        Value::Unknown
    );
}

#[test]
fn c2_expiry_without_version_change_is_explicit_and_propagates() {
    let g = graph(
        &["sensor"],
        &["derived", "action"],
        vec![
            rule("r1", &["sensor"], 1, "derived"),
            rule("r2", &["derived"], 1, "action"),
        ],
    );
    let mut runtime = AssuranceRuntime::new(g);
    let mut observation = atom("sensor", Status::Valid, 2);
    observation.version = 10;
    runtime.update(observation).unwrap();
    runtime.advance_to(t(1)).unwrap();
    assert_eq!(
        runtime.evaluation().assurance_by_node["action"],
        Value::Valid { horizon: t(2) }
    );
    runtime.advance_to(t(2)).unwrap();
    assert_eq!(runtime.evidence()["sensor"].status, Status::Unknown);
    assert_eq!(runtime.evidence()["sensor"].version, 10);
    assert_eq!(runtime.epoch(), 2);
    for id in ["sensor", "derived", "action"] {
        assert_eq!(runtime.evaluation().assurance_by_node[id], Value::Unknown);
        assert!(
            !runtime
                .evaluation()
                .preferred_witness_by_node
                .contains_key(id)
        );
    }
    let count = runtime.audit().len();
    runtime.advance_to(t(3)).unwrap();
    assert_eq!(runtime.audit().len(), count);
    let events: Vec<_> = runtime
        .audit()
        .iter()
        .filter(|entry| matches!(entry.event, AuditEvent::EvidenceExpired { .. }))
        .collect();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].at, t(2));
    assert_eq!(
        events[0].event,
        AuditEvent::EvidenceExpired {
            evidence_id: "sensor".into(),
            version: 10
        }
    );
}

#[test]
fn and_or_truth_tables_cover_all_pairs() {
    use Status::{Invalid as I, Unknown as U, Valid as V};
    // Inputs, AND, OR: explicit strong three-valued truth table.
    let cases = [
        (V, V, V, V),
        (V, U, U, V),
        (V, I, I, V),
        (U, V, U, V),
        (U, U, U, U),
        (U, I, I, U),
        (I, V, I, V),
        (I, U, I, U),
        (I, I, I, I),
    ];
    for (a, b, and, or) in cases {
        let g = graph(
            &["a", "b"],
            &["and", "or"],
            vec![
                rule("and", &["a", "b"], 2, "and"),
                rule("or", &["a", "b"], 1, "or"),
            ],
        );
        let evidence =
            BTreeMap::from([("a".into(), atom("a", a, 5)), ("b".into(), atom("b", b, 8))]);
        let result = evaluate_full(&g, &evidence, t(0));
        assert_eq!(result.assurance_by_node["and"].status(), and);
        assert_eq!(result.assurance_by_node["or"].status(), or);
    }
}

#[test]
fn two_of_three_truth_table_including_unknown_and_invalid() {
    use Status::{Invalid as I, Unknown as U, Valid as V};
    let cases = [
        ([V, V, V], V),
        ([V, V, U], V),
        ([V, V, I], V),
        ([V, U, U], U),
        ([V, U, I], U),
        ([V, I, I], I),
        ([U, U, U], U),
        ([U, U, I], U),
        ([U, I, I], I),
        ([I, I, I], I),
    ];
    for (states, expected) in cases {
        for order in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            let evidence = ["s1", "s2", "thermal"]
                .iter()
                .enumerate()
                .map(|(i, id)| ((*id).into(), atom(id, states[order[i]], 10)))
                .collect();
            assert_eq!(
                evaluate_full(&quorum(), &evidence, t(0)).assurance_by_node["hazard"].status(),
                expected
            );
        }
    }
}

#[test]
fn strongest_threshold_support_sets_the_horizon() {
    let evidence = BTreeMap::from([
        ("s1".into(), atom("s1", Status::Valid, 5)),
        ("s2".into(), atom("s2", Status::Valid, 8)),
        ("thermal".into(), atom("thermal", Status::Valid, 12)),
    ]);
    let result = evaluate_full(&quorum(), &evidence, t(0));
    assert_eq!(
        result.assurance_by_node["hazard"],
        Value::Valid { horizon: t(8) }
    );
    assert_eq!(
        result.preferred_witness_by_node["hazard"].evidence_ids,
        ids(&["s2", "thermal"])
    );
}

#[test]
fn alternative_support_prefers_horizon_then_size_then_rule_id() {
    let g = graph(
        &["a", "b", "c"],
        &["out"],
        vec![
            rule("z-small", &["a"], 1, "out"),
            rule("a-small", &["b"], 1, "out"),
            rule("0-large", &["a", "b"], 2, "out"),
            rule("later", &["c"], 1, "out"),
        ],
    );
    let mut evidence = BTreeMap::from([
        ("a".into(), atom("a", Status::Valid, 8)),
        ("b".into(), atom("b", Status::Valid, 8)),
        ("c".into(), atom("c", Status::Valid, 12)),
    ]);
    let selected = |e: &BTreeMap<String, EvidenceAtom>| {
        evaluate_full(&g, e, t(0)).preferred_witness_by_node["out"]
            .justification_id
            .clone()
            .unwrap()
    };
    assert_eq!(selected(&evidence), "later");
    evidence.remove("c");
    assert_eq!(selected(&evidence), "a-small");
}

#[test]
fn alternative_invalid_and_unknown_results_are_not_conflated() {
    let g = graph(
        &["a", "b"],
        &["out"],
        vec![rule("a", &["a"], 1, "out"), rule("b", &["b"], 1, "out")],
    );
    let mut evidence = BTreeMap::from([("a".into(), atom("a", Status::Invalid, 8))]);
    assert_eq!(
        evaluate_full(&g, &evidence, t(0)).assurance_by_node["out"],
        Value::Unknown
    );
    evidence.insert("b".into(), atom("b", Status::Invalid, 8));
    assert_eq!(
        evaluate_full(&g, &evidence, t(0)).assurance_by_node["out"],
        Value::Invalid
    );
}

#[test]
fn diamond_graph_deduplicates_shared_evidence_and_evaluates_every_rule() {
    let g = graph(
        &["a"],
        &["left", "right", "out"],
        vec![
            rule("l", &["a"], 1, "left"),
            rule("r", &["a"], 1, "right"),
            rule("both", &["left", "right"], 2, "out"),
        ],
    );
    let result = evaluate_full(
        &g,
        &BTreeMap::from([("a".into(), atom("a", Status::Valid, 5))]),
        t(0),
    );
    assert_eq!(
        result.preferred_witness_by_node["out"].evidence_ids,
        ids(&["a"])
    );
    assert_eq!(result.assurance_by_justification.len(), 3);
    assert_eq!(g.justifications_by_premise()["a"], vec!["l", "r"]);
    assert_eq!(
        g.premises_by_justification("both").unwrap(),
        ["left", "right"]
    );
}

#[test]
fn malformed_graphs_are_rejected() {
    let nodes = || {
        vec![
            Node::new("a", NodeKind::Evidence),
            Node::new("out", NodeKind::Derived),
        ]
    };
    for threshold in [0, 2] {
        assert_eq!(
            AssuranceGraph::new(nodes(), vec![rule("r", &["a"], threshold, "out")]).unwrap_err(),
            GraphError::InvalidThreshold("r".into())
        );
    }
    assert_eq!(
        AssuranceGraph::new(nodes(), vec![rule("r", &["a", "a"], 2, "out")]).unwrap_err(),
        GraphError::DuplicatePremise("r".into())
    );
    assert_eq!(
        AssuranceGraph::new(nodes(), vec![rule("r", &["missing"], 1, "out")]).unwrap_err(),
        GraphError::UnknownNode("missing".into())
    );
    assert_eq!(
        AssuranceGraph::new(nodes(), vec![rule("r", &["a"], 1, "missing")]).unwrap_err(),
        GraphError::UnknownNode("missing".into())
    );
    assert_eq!(
        AssuranceGraph::new(nodes(), vec![rule("r", &["a"], 1, "a")]).unwrap_err(),
        GraphError::EvidenceConclusion("a".into())
    );
    assert_eq!(
        AssuranceGraph::new(nodes(), vec![]).unwrap_err(),
        GraphError::UnsupportedConclusion("out".into())
    );
    assert_eq!(
        AssuranceGraph::new(
            vec![
                Node::new("a", NodeKind::Evidence),
                Node::new("a", NodeKind::Evidence)
            ],
            vec![]
        )
        .unwrap_err(),
        GraphError::DuplicateNode("a".into())
    );
    assert_eq!(
        AssuranceGraph::new(
            nodes(),
            vec![rule("r", &["a"], 1, "out"), rule("r", &["a"], 1, "out")]
        )
        .unwrap_err(),
        GraphError::DuplicateJustification("r".into())
    );
    assert_eq!(
        AssuranceGraph::new(vec![Node::new(" ", NodeKind::Evidence)], vec![]).unwrap_err(),
        GraphError::EmptyId
    );
    assert_eq!(
        AssuranceGraph::new(nodes(), vec![rule("", &["a"], 1, "out")]).unwrap_err(),
        GraphError::EmptyId
    );
    assert_eq!(
        AssuranceGraph::new(nodes(), vec![rule("r", &["out"], 1, "out")]).unwrap_err(),
        GraphError::Cycle
    );
    assert_eq!(
        AssuranceGraph::new(
            vec![
                Node::new("x", NodeKind::Derived),
                Node::new("y", NodeKind::Derived)
            ],
            vec![rule("x", &["y"], 1, "x"), rule("y", &["x"], 1, "y")]
        )
        .unwrap_err(),
        GraphError::Cycle
    );
}

#[test]
fn all_node_kinds_are_evaluated_as_separate_roots() {
    let kinds = [
        NodeKind::Derived,
        NodeKind::ActionStart,
        NodeKind::ActionRun,
        NodeKind::ActionCommit,
        NodeKind::ActionOutcome,
        NodeKind::PhysicalSafety,
    ];
    let mut nodes = vec![Node::new("a", NodeKind::Evidence)];
    let mut rules = Vec::new();
    for (i, kind) in kinds.into_iter().enumerate() {
        let id = format!("root{i}");
        nodes.push(Node::new(&id, kind));
        rules.push(rule(&id, &["a"], 1, &id));
    }
    let g = AssuranceGraph::new(nodes, rules).unwrap();
    let result = evaluate_full(&g, &BTreeMap::new(), t(0));
    assert_eq!(result.assurance_by_node.len(), 7);
    assert!(
        result
            .assurance_by_node
            .values()
            .all(|v| *v == Value::Unknown)
    );
}

#[test]
fn replacement_does_not_expire_at_the_old_deadline() {
    let mut runtime = AssuranceRuntime::new(graph(&["a"], &[], vec![]));
    runtime.update(atom("a", Status::Valid, 2)).unwrap();
    runtime.advance_to(t(1)).unwrap();
    let mut replacement = atom("a", Status::Valid, 10);
    replacement.version = 2;
    replacement.observed_at = t(1);
    runtime.update(replacement).unwrap();
    runtime.advance_to(t(2)).unwrap();
    assert_eq!(
        runtime.evaluation().assurance_by_node["a"],
        Value::Valid { horizon: t(10) }
    );
    assert!(
        !runtime
            .audit()
            .iter()
            .any(|entry| matches!(entry.event, AuditEvent::EvidenceExpired { .. }))
    );
    runtime.advance_to(t(10)).unwrap();
    assert_eq!(runtime.evidence()["a"].version, 2);
    assert_eq!(runtime.evidence()["a"].status, Status::Unknown);
}

#[test]
fn rejected_updates_leave_state_epoch_and_audit_unchanged() {
    let mut runtime = AssuranceRuntime::new(graph(&["a"], &[], vec![]));
    runtime.update(atom("a", Status::Valid, 5)).unwrap();
    let before = (
        runtime.evidence().clone(),
        runtime.evaluation().clone(),
        runtime.audit().to_vec(),
        runtime.epoch(),
    );
    let mut future = atom("a", Status::Valid, 5);
    future.version = 2;
    future.observed_at = t(1);
    let mut malformed = atom("a", Status::Valid, 5);
    malformed.version = 2;
    malformed.source_id.clear();
    let mut different_source = atom("a", Status::Valid, 5);
    different_source.version = 2;
    different_source.source_id = "other".into();
    for bad in [
        atom("a", Status::Valid, 5),
        atom("missing", Status::Valid, 5),
        future,
        malformed,
        different_source,
    ] {
        assert!(runtime.update(bad).is_err());
        assert_eq!(
            (
                runtime.evidence().clone(),
                runtime.evaluation().clone(),
                runtime.audit().to_vec(),
                runtime.epoch()
            ),
            before
        );
    }
}

#[test]
fn late_expired_observation_never_yields_valid_support() {
    let mut runtime = AssuranceRuntime::new(graph(&["a"], &[], vec![]));
    runtime.advance_to(t(3)).unwrap();
    runtime.update(atom("a", Status::Valid, 2)).unwrap();
    assert_eq!(runtime.evaluation().assurance_by_node["a"], Value::Unknown);
    assert_eq!(runtime.epoch(), 2);
    assert_eq!(runtime.audit().len(), 2); // Observation accepted, then explicit expiry.
    assert!(matches!(
        runtime.audit()[1].event,
        AuditEvent::EvidenceExpired { .. }
    ));
    assert_eq!(runtime.audit()[1].at, t(3));
}

#[test]
fn simultaneous_expiries_are_logged_before_recomputation() {
    let mut runtime = AssuranceRuntime::new(graph(
        &["a", "b"],
        &["out"],
        vec![rule("r", &["a", "b"], 1, "out")],
    ));
    runtime.update(atom("b", Status::Valid, 2)).unwrap();
    runtime.update(atom("a", Status::Valid, 2)).unwrap();
    let start = runtime.audit().len();
    runtime.advance_to(t(10)).unwrap();
    let events = &runtime.audit()[start..];
    assert_eq!(
        events[0].event,
        AuditEvent::EvidenceExpired {
            evidence_id: "a".into(),
            version: 1
        }
    );
    assert_eq!(
        events[1].event,
        AuditEvent::EvidenceExpired {
            evidence_id: "b".into(),
            version: 1
        }
    );
    assert!(events.iter().all(|entry| entry.at == t(2)));
    assert_eq!(runtime.now(), t(10));
}

#[test]
fn invalid_and_unknown_do_not_generate_expiry_events() {
    let mut runtime = AssuranceRuntime::new(graph(&["a", "b"], &[], vec![]));
    runtime.update(atom("a", Status::Invalid, 2)).unwrap();
    runtime.update(atom("b", Status::Unknown, 2)).unwrap();
    let count = runtime.audit().len();
    runtime.advance_to(t(10)).unwrap();
    assert_eq!(runtime.audit().len(), count);
    assert_eq!(runtime.evaluation().assurance_by_node["a"], Value::Invalid);
}

#[test]
fn backward_and_overflowing_time_are_rejected_without_mutation() {
    let mut runtime = AssuranceRuntime::new(graph(&[], &[], vec![]));
    runtime.advance_to(t(2)).unwrap();
    assert_eq!(
        runtime.advance_to(t(1)),
        Err(RuntimeError::TimeWentBackwards)
    );
    assert_eq!(
        runtime.advance(Duration::MAX),
        Err(RuntimeError::TimeOverflow)
    );
    assert_eq!(runtime.now(), t(2));
}

#[test]
fn graph_input_order_does_not_change_results_or_audit() {
    let nodes = vec![
        Node::new("b", NodeKind::Evidence),
        Node::new("a", NodeKind::Evidence),
        Node::new("out", NodeKind::Derived),
    ];
    let rules = vec![
        rule("z", &["b", "a"], 1, "out"),
        rule("a", &["a"], 1, "out"),
    ];
    let mut reversed_nodes = nodes.clone();
    reversed_nodes.reverse();
    let mut reversed_rules = rules.clone();
    reversed_rules.reverse();
    reversed_rules[1].premises.reverse();
    let mut first = AssuranceRuntime::new(AssuranceGraph::new(nodes, rules).unwrap());
    let mut second =
        AssuranceRuntime::new(AssuranceGraph::new(reversed_nodes, reversed_rules).unwrap());
    for runtime in [&mut first, &mut second] {
        runtime.update(atom("a", Status::Valid, 2)).unwrap();
        runtime.update(atom("b", Status::Valid, 4)).unwrap();
        runtime.advance_to(t(5)).unwrap();
    }
    assert_eq!(first.evaluation(), second.evaluation());
    assert_eq!(first.audit(), second.audit());
}

#[test]
fn nested_horizon_uses_the_earliest_required_leaf() {
    let g = graph(
        &["a", "b", "c", "healthy"],
        &["quorum", "start"],
        vec![
            rule("quorum", &["a", "b", "c"], 2, "quorum"),
            rule("start", &["quorum", "healthy"], 2, "start"),
        ],
    );
    let evidence = [("a", 5), ("b", 8), ("c", 12), ("healthy", 6)]
        .into_iter()
        .map(|(id, expiry)| (id.into(), atom(id, Status::Valid, expiry)))
        .collect();
    let result = evaluate_full(&g, &evidence, t(0));
    assert_eq!(
        result.assurance_by_node["quorum"],
        Value::Valid { horizon: t(8) }
    );
    assert_eq!(
        result.assurance_by_node["start"],
        Value::Valid { horizon: t(6) }
    );
    assert_eq!(
        result.preferred_witness_by_node["start"].evidence_ids,
        ids(&["b", "c", "healthy"])
    );
}

#[test]
fn clock_jump_and_stepwise_expiry_produce_identical_histories() {
    let g = graph(
        &["a", "b"],
        &["out"],
        vec![rule("r", &["a", "b"], 1, "out")],
    );
    let mut jump = AssuranceRuntime::new(g.clone());
    let mut steps = AssuranceRuntime::new(g);
    for runtime in [&mut jump, &mut steps] {
        runtime.update(atom("a", Status::Valid, 2)).unwrap();
        runtime.update(atom("b", Status::Valid, 4)).unwrap();
    }
    jump.advance_to(t(5)).unwrap();
    for time in 1..=5 {
        steps.advance_to(t(time)).unwrap();
    }
    assert_eq!(jump.audit(), steps.audit());
    assert_eq!(jump.evaluation(), steps.evaluation());
}

#[test]
fn new_observation_restores_support_after_expiry() {
    let mut runtime =
        AssuranceRuntime::new(graph(&["a"], &["out"], vec![rule("r", &["a"], 1, "out")]));
    runtime.update(atom("a", Status::Valid, 2)).unwrap();
    runtime.advance_to(t(3)).unwrap();
    let mut fresh = atom("a", Status::Valid, 8);
    fresh.version = 2;
    fresh.observed_at = t(3);
    runtime.update(fresh).unwrap();
    assert_eq!(
        runtime.evaluation().assurance_by_node["out"],
        Value::Valid { horizon: t(8) }
    );
    assert_eq!(runtime.epoch(), 3);
    let mut bad_time = atom("a", Status::Valid, 2);
    bad_time.version = 3;
    bad_time.observed_at = t(3);
    assert_eq!(
        runtime.update(bad_time),
        Err(RuntimeError::InvalidObservationTime)
    );
    assert_eq!(runtime.epoch(), 3);
}

#[test]
fn evidence_loss_only_changes_action_roots_that_require_it() {
    let graph = AssuranceGraph::new(
        vec![
            Node::new("hazard", NodeKind::Evidence),
            Node::new("healthy", NodeKind::Evidence),
            Node::new("fan.start", NodeKind::ActionStart),
            Node::new("alarm.start", NodeKind::ActionStart),
        ],
        vec![
            rule("fan", &["hazard", "healthy"], 2, "fan.start"),
            rule("alarm", &["hazard"], 1, "alarm.start"),
        ],
    )
    .unwrap();
    let mut runtime = AssuranceRuntime::new(graph);
    runtime.update(atom("hazard", Status::Valid, 10)).unwrap();
    runtime.update(atom("healthy", Status::Valid, 10)).unwrap();
    let alarm_witness = runtime.evaluation().preferred_witness_by_node["alarm.start"].clone();
    assert_eq!(
        runtime.evaluation().assurance_by_node["fan.start"],
        Value::Valid { horizon: t(10) }
    );
    let mut lost = atom("healthy", Status::Unknown, 10);
    lost.version = 2;
    runtime.update(lost).unwrap();
    assert_eq!(
        runtime.evaluation().assurance_by_node["fan.start"],
        Value::Unknown
    );
    assert_eq!(
        runtime.evaluation().assurance_by_node["alarm.start"],
        Value::Valid { horizon: t(10) }
    );
    assert_eq!(
        runtime.evaluation().preferred_witness_by_node["alarm.start"],
        alarm_witness
    );
}
