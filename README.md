# Dependency-Closed Runtime Assurance

This Rust library implements the **Week 1 evidence lifecycle and full reference
evaluator** from the Agentic IoT Six Week Research Timeline, using Implementation
Specification v1 sections 4–14, 16 and 32. It models declared evidence support;
it does not establish sensor truth or physical safety, or dispatch device actions.

A validated dependency graph expresses AND, OR and k-of-n justifications. Each
valid conclusion has a preferred witness identifying its supporting observations
and a horizon showing when that support expires. A deterministic runtime accepts
versioned observations, processes explicit expiry events, and records transitions.

## Run and inspect

With a Rust toolchain supporting edition 2024 (no external dependencies):

```sh
cargo run --offline --example week_one
cargo test --offline
cargo fmt --check
cargo clippy --offline --all-targets -- -D warnings
```

The `week_one` example demonstrates:

- **C1:** a 2-of-3 hazard quorum stays valid after one sensor becomes unknown,
  with the supporting witness changing to the remaining two sensors.
- **C2:** evidence version 10 expires at simulated time 2 without a new sensor
  message. At time 3 its status is unknown, its version is still 10, and the
  explicit expiry event is recorded at time 2.

Read [schema and semantic notes](docs/week-one.md) for the type contracts,
three-valued truth tables, assumptions, tie-breaking, event ordering, and
acceptance-test mapping. See `examples/week_one.rs` for the public API usage.

## Implementation

| Module | Responsibility |
| --- | --- |
| `evidence` | Three-valued status, assurance values, complete evidence metadata |
| `graph` | Validated immutable DAG, topological order, premise/conclusion indexes |
| `evaluator` | Full evaluation of every rule and node, preferred witnesses and horizons |
| `runtime` | Versioned updates, epochs, controlled time, expiry and audit history |
| `clock` | Monotonic virtual clock; no real sleeping or wall-clock decisions |

Use `AssuranceRuntime` for evidence updates and expiry processing. `AssuranceStatus`
is the single status enum used by evidence and derived assurance values.
The full evaluator remains the reference for verifying incremental evaluation.

## Week 1 completion gate

`tests/week_one.rs` verifies hand-worked truth tables and horizons, C1/C2,
malformed graph rejection, nested support, deterministic ties and replay,
explicit expiry at the deadline, simultaneous expiries, replacement observations,
and atomic rejection of malformed or stale updates. No test uses real sleeping.

Week 2 adds a tiny-graph exhaustive witness oracle and incremental evaluation
checked against this full evaluator. Leases, action execution, the building
simulator and AI integration remain later work. The agreed language split remains
Rust for the runtime and Python for the later planner adapter and analysis.
