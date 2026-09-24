//! Validated, immutable assurance DAG and its lookup indexes.
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Evidence,
    Derived,
    ActionStart,
    ActionRun,
    ActionCommit,
    ActionOutcome,
    PhysicalSafety,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub node_id: String,
    pub kind: NodeKind,
}

impl Node {
    pub fn new(id: impl Into<String>, kind: NodeKind) -> Self {
        Self {
            node_id: id.into(),
            kind,
        }
    }
}

/// AND is threshold = premises.len(); OR is threshold = 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Justification {
    pub justification_id: String,
    pub premises: Vec<String>,
    pub threshold: usize,
    pub conclusion: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphError {
    EmptyId,
    DuplicateNode(String),
    DuplicateJustification(String),
    UnknownNode(String),
    InvalidThreshold(String),
    DuplicatePremise(String),
    EvidenceConclusion(String),
    UnsupportedConclusion(String),
    Cycle,
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid assurance graph: {self:?}")
    }
}
impl std::error::Error for GraphError {}

#[derive(Debug, Clone)]
pub struct AssuranceGraph {
    nodes: BTreeMap<String, NodeKind>,
    justifications: BTreeMap<String, Justification>,
    by_premise: BTreeMap<String, Vec<String>>,
    by_conclusion: BTreeMap<String, Vec<String>>,
    order: Vec<String>,
}

impl AssuranceGraph {
    pub fn new(nodes: Vec<Node>, rules: Vec<Justification>) -> Result<Self, GraphError> {
        let mut graph = Self {
            nodes: BTreeMap::new(),
            justifications: BTreeMap::new(),
            by_premise: BTreeMap::new(),
            by_conclusion: BTreeMap::new(),
            order: Vec::new(),
        };
        for node in nodes {
            if node.node_id.trim().is_empty() {
                return Err(GraphError::EmptyId);
            }
            if graph
                .nodes
                .insert(node.node_id.clone(), node.kind)
                .is_some()
            {
                return Err(GraphError::DuplicateNode(node.node_id));
            }
        }
        let mut dependencies: BTreeMap<String, BTreeSet<String>> = graph
            .nodes
            .keys()
            .map(|id| (id.clone(), BTreeSet::new()))
            .collect();
        let mut successors = dependencies.clone();
        for mut rule in rules {
            let id = &rule.justification_id;
            if id.trim().is_empty() {
                return Err(GraphError::EmptyId);
            }
            if graph.justifications.contains_key(id) {
                return Err(GraphError::DuplicateJustification(id.clone()));
            }
            if rule.threshold == 0 || rule.threshold > rule.premises.len() {
                return Err(GraphError::InvalidThreshold(id.clone()));
            }
            match graph.nodes.get(&rule.conclusion) {
                None => return Err(GraphError::UnknownNode(rule.conclusion)),
                Some(NodeKind::Evidence) => {
                    return Err(GraphError::EvidenceConclusion(rule.conclusion));
                }
                _ => {}
            }
            rule.premises.sort();
            for pair in rule.premises.windows(2) {
                if pair[0] == pair[1] {
                    return Err(GraphError::DuplicatePremise(id.clone()));
                }
            }
            for premise in &rule.premises {
                if !graph.nodes.contains_key(premise) {
                    return Err(GraphError::UnknownNode(premise.clone()));
                }
                graph
                    .by_premise
                    .entry(premise.clone())
                    .or_default()
                    .push(id.clone());
                dependencies
                    .get_mut(&rule.conclusion)
                    .unwrap()
                    .insert(premise.clone());
                successors
                    .get_mut(premise)
                    .unwrap()
                    .insert(rule.conclusion.clone());
            }
            graph
                .by_conclusion
                .entry(rule.conclusion.clone())
                .or_default()
                .push(id.clone());
            graph.justifications.insert(id.clone(), rule);
        }
        for (id, kind) in &graph.nodes {
            if *kind != NodeKind::Evidence && !graph.by_conclusion.contains_key(id) {
                return Err(GraphError::UnsupportedConclusion(id.clone()));
            }
        }
        for ids in graph
            .by_premise
            .values_mut()
            .chain(graph.by_conclusion.values_mut())
        {
            ids.sort();
        }
        let mut ready: BTreeSet<String> = dependencies
            .iter()
            .filter(|(_, deps)| deps.is_empty())
            .map(|(id, _)| id.clone())
            .collect();
        while let Some(id) = ready.pop_first() {
            graph.order.push(id.clone());
            for next in &successors[&id] {
                let deps = dependencies.get_mut(next).unwrap();
                deps.remove(&id);
                if deps.is_empty() {
                    ready.insert(next.clone());
                }
            }
        }
        if graph.order.len() != graph.nodes.len() {
            return Err(GraphError::Cycle);
        }
        Ok(graph)
    }

    pub fn nodes(&self) -> &BTreeMap<String, NodeKind> {
        &self.nodes
    }
    pub fn justifications(&self) -> &BTreeMap<String, Justification> {
        &self.justifications
    }
    pub fn topological_order(&self) -> &[String] {
        &self.order
    }
    pub fn justifications_by_premise(&self) -> &BTreeMap<String, Vec<String>> {
        &self.by_premise
    }
    pub fn justifications_by_conclusion(&self) -> &BTreeMap<String, Vec<String>> {
        &self.by_conclusion
    }
    pub fn premises_by_justification(&self, id: &str) -> Option<&[String]> {
        self.justifications
            .get(id)
            .map(|rule| rule.premises.as_slice())
    }
}
