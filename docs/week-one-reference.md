# Week 1 technical reference

For the plain-language explanation, start with [Understanding Week 1](week-one.md).

This is the implementation record for the existing timeline, not a replacement
research specification. The supplied Implementation Specification v1 is the
technical authority. Its referenced Formal Models v1/v2 were not available among
the supplied documents; the choices below make otherwise implicit behavior
explicit and testable pending comparison with those models.

## Public data and interfaces

All safety times are `std::time::Duration` from the virtual clock's zero, with
nanosecond precision. Horizons are exclusive: support is valid only at `now <
horizon`. Wall-clock labels are not collected in this deterministic kernel;
a future host adapter may attach them for human logs, never for expiry decisions.

| Type | Fields / invariant |
| --- | --- |
| `AssuranceStatus` | `Valid`, `Unknown`, `Invalid`; no Boolean conversion |
| `AssuranceValue` | `Valid { horizon }`, `Unknown`, `Invalid`; the latter two cannot carry a horizon |
| `EvidenceAtom` | `evidence_id`, `evidence_type`, `predicate`, `source_id`, `version: u64`, `observed_at`, `expires_at`, `status`, optional `payload_hash` |
| `Node` | `node_id`, `kind` |
| `NodeKind` | Evidence, Derived, ActionStart, ActionRun, ActionCommit, ActionOutcome, PhysicalSafety |
| `Justification` | `justification_id`, distinct `premises`, `threshold`, `conclusion` |
| `Witness` | `node_id`, deduplicated ordered `evidence_ids`, `horizon`, selected `justification_id`, selected `premise_ids` |
| `Evaluation` | `assurance_by_node`, `assurance_by_justification`, `preferred_witness_by_node` |
| `AuditEntry` | Monotonic `at`, assurance `epoch`, typed `event` |

Action phase kinds are distinct graph roots, not action state machines. A valid
start root is an assurance result, not a durable lease or permission to dispatch.

`AssuranceGraph::new(nodes, justifications)` validates configuration and returns
an immutable graph or `GraphError`. Read-only accessors expose rules (including
premises by justification), rules by premise, rules by conclusion, and the
stable topological order. No lease index is instantiated before leases exist.

`evaluate_full(&graph, &evidence_map, now)` returns a new `Evaluation`, visiting
every node and every justification. It is a pure snapshot evaluator, not an
event processor. The runtime supplies validated atoms keyed by their ID. Missing
or mismatched map identities fail closed to unknown; extra map entries are ignored.
Standalone callers are responsible for metadata validation. Runtime clients
should read `runtime.evaluation()` rather than bypass the lifecycle.

`AssuranceRuntime::new(graph)` starts at time/epoch zero with all observations
missing and a fully evaluated unknown graph. Its graph, evidence, evaluation,
audit and time are exposed read-only. `update(atom)` accepts one observation;
`advance(elapsed)` and `advance_to(target)` process time and due expiry events.
All three operations return typed errors for rejected input.

## Configuration mapping (specification section 32)

Week 1 exposes typed Rust configuration and documents the mapping below. It does
not include a JSON parser, network protocol or source authentication layer.

```json
{
  "id": "zone2.gas_sensor_1.high",
  "type": "sensor",
  "predicate": "gas_high",
  "source": "gas_sensor_1",
  "max_age_ms": 2000
}
```

Create an Evidence node using `id`; observations map `id/type/source` to
`evidence_id/evidence_type/source_id`. A future ingestion adapter computes
`expires_at = observed_at + max_age_ms` with checked arithmetic. Runtime atoms
already carry the resulting absolute deadline; raw readings may be stored
outside the kernel. Status is supplied by the domain-specific observation
validator, not inferred from a label or payload hash.

```json
{
  "id": "zone2.hazard_quorum",
  "premises": ["zone2.gas_sensor_1.high", "zone2.gas_sensor_2.high", "zone2.thermal.high"],
  "threshold": 2,
  "conclusion": "zone2.hazard_confirmed"
}
```

Declare the three Evidence nodes and a Derived conclusion. Map this rule's `id`
to `justification_id`; the remaining fields map directly. All node references
must exist. Empty IDs, duplicate node/rule IDs, duplicate premises, zero or
oversized thresholds, rules concluding at Evidence nodes, derived nodes without
justifications, self-cycles and multi-node cycles are rejected. An empty graph
and evidence-only graphs are valid. A configured sensor with no observation is
unknown; a reference to an unconfigured sensor is a configuration error.

## Evaluation and witness semantics

The implemented threshold truth rule is strong three-valued logic. For threshold
k, let v be valid premises and u be unknown premises:

- VALID if v >= k.
- INVALID if v + u < k: even resolving every unknown positively cannot meet k.
- UNKNOWN otherwise.

This is an explicit implementation assumption where the supplied specification
requires three values but does not spell out the whole truth table. INVALID
means the predicate lacks the required support under this logic; it does not
mean that the physical situation is undesirable. UNKNOWN is never treated as VALID.

| A | B | AND | OR |
| --- | --- | --- | --- |
| VALID | VALID | VALID | VALID |
| VALID | UNKNOWN | UNKNOWN | VALID |
| VALID | INVALID | INVALID | VALID |
| UNKNOWN | UNKNOWN | UNKNOWN | UNKNOWN |
| UNKNOWN | INVALID | INVALID | UNKNOWN |
| INVALID | INVALID | INVALID | INVALID |

Rows are symmetric. AND uses k=n; OR uses k=1. Multiple justifications for one
conclusion behave as OR: any valid rule suffices, otherwise any unknown rule
makes the result unknown, otherwise it is invalid.

Atomic valid witnesses contain one evidence ID and its expiry. For a threshold,
choose k valid premise witnesses ordered by higher horizon, fewer evidence atoms,
then lexicographically smaller premise node ID. The last criterion completes the
specification's unspecified tie between premises. Union their leaf evidence IDs;
the result's horizon is the minimum selected horizon. Shared leaves appear once.
Thresholds count distinct configured premise nodes, not independent physical
sensors: independence must be established by the model/configuration.

Among alternative valid justifications, prefer higher horizon, then fewer atoms
in the candidate union, then lexicographically smaller justification ID, as in
section 16. This is deterministic local witness selection. It does not claim a
globally minimum-cardinality leaf union across every possible tied combination.
The Week 2 exhaustive oracle will check maximal horizons on tiny graphs.

Hand-worked example: a 2-of-3 gate with horizons 5, 8, 12 chooses the latter two
and has horizon 8. A downstream AND with another premise expiring at 6 has horizon
6. A horizon is a limit on declared support, not a sensor-truth or physical-safety
guarantee. Evidence version/provenance can be inspected through the runtime store
and observation audit entries; witnesses are not version-bound leases.

## Evidence lifecycle, epochs and audit

The runtime rejects unknown/non-evidence IDs, blank source/type/predicate, future
observations, expiry preceding observation, and non-increasing per-ID versions.
Version zero is allowed initially. Once an ID is observed, its source/type/
predicate identity is fixed for this runtime; changing that identity requires a
new ID/configuration. These are explicit ingestion policies. Source clock
translation and restart/version-reset handling belong to a later adapter.

Accepted updates increment the epoch even when the status is unchanged, because
new observation versions matter. `EvidenceUpdated` records the previous atom
and the complete incoming atom. Rejected operations do not alter time, epochs,
evidence, evaluation or audit history.

A VALID atom expires at its deadline: increment the epoch, record
`EvidenceExpired { evidence_id, version }`, and change its logical status to
UNKNOWN without changing its version or observation timestamps. Original
observation status remains available in `EvidenceUpdated`. UNKNOWN and INVALID
observations do not generate expiry events. A new observation is needed to
replace their state.

Advancing across several deadlines processes them chronologically. At a shared
deadline, all expiries are recorded in evidence-ID order before full reevaluation;
each expiry has its own epoch and the resulting evaluation uses the final epoch
of the group. This prevents conclusions from observing partially processed
simultaneous expiry events. Clock advancement without an event does not increment
the epoch. Backward time and arithmetic overflow are rejected.

Pending deadlines are derived from current atoms rather than a separate timer
queue, so an overwritten observation cannot leave a stale timer. Already-expired
VALID arrivals are accepted and immediately explicitly expired at ingestion time,
using two epochs and one final evaluation; they never publish a valid snapshot.
At a refresh exactly on an old deadline, advance time first: expiry is processed
before the replacement observation. Repeated advancement cannot re-emit expiry.

Each reevaluation logs `AssuranceChanged` for changes in status or horizon, and
`WitnessChanged` for acquisition, replacement or removal of support. Both carry
old/new values. Initial all-unknown state is the documented epoch-zero baseline.
The log is an in-memory, deterministic audit history, not durable storage. The
runtime uses no wall-clock reads, background tasks or real sleeping.

## Acceptance evidence and next boundary

| Week 1 requirement | Automated check |
| --- | --- |
| C1 sensor loss | `c1_quorum_survives_one_sensor_loss`, including comparison with an all-sensors-required graph rule |
| C2 expiry without new version | `c2_expiry_without_version_change_is_explicit_and_propagates` |
| Hand-worked logical cases | AND/OR pair table and all permutations of 2-of-3 status combinations |
| Witnesses and horizons | Strongest threshold support, alternative ties, shared-leaf diamond, nested horizon tests |
| Graph validation | Malformed thresholds, IDs, references, duplicate premises, unsupported conclusions and cycles |
| Event lifecycle | Replacement, simultaneous expiry, late arrivals, rejected updates, restoration and chronological replay |
| Reproducibility | Reversed graph input order yields identical results and audit entries |
| Independent action roots | Evidence loss affects only roots whose rules require that evidence |

The full evaluator and reverse indexes are ready for Week 2's incremental
implementation. Exhaustive witness enumeration, randomized full/incremental
comparison, leases, device dispatch, physical simulation and the Python AI
planner are intentionally outside the Week 1 gate.
