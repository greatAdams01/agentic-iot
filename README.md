# Dependency-Closed Runtime Assurance

An action's permission should depend on evidence that is still usable when the
decision is made. For example, a door-closing action requires evidence that the
room is clear. Once that evidence expires, the action must be blocked.

This Rust library is **milestone one**, a small reference implementation of that
rule. It checks declared evidence support; it does not guarantee physical safety
or control any device. There are no external dependencies.

## Run

From this directory, with a Rust toolchain supporting edition 2024:

```sh
cargo test --offline
cargo run --offline --example expiry
cargo fmt --check
cargo clippy --offline --all-targets -- -D warnings
```

The example allows `close_door` at time zero and blocks it at five seconds, when
`room_clear` expires. Time advances explicitly; the program never sleeps.

## Current semantics

- Each evidence record has a declared VALID, UNKNOWN, or INVALID status and an
  absolute expiry deadline measured from the simulation clock's zero.
- VALID evidence is usable only while `now < expires_at`. At and after expiry,
  its effective status is UNKNOWN. The stored observation is not mutated.
- UNKNOWN and INVALID remain unsupported until replaced with new evidence.
- Every evidence item listed in an action contract must currently be VALID.
- Missing evidence blocks the action. Empty contracts also block, preventing
  accidental unconditional permission in this milestone.
- Every call recomputes all contracts and returns reasons for blocked actions.
  Decisions are snapshots, not durable execution permissions. Advancing the clock
  does not run an evaluator in the background: call `evaluate_all` again.
- There are no derived evidence nodes yet; requirements refer directly to records.

## Layout

| Path | Responsibility |
| --- | --- |
| `src/clock.rs` | Controlled monotonic simulation time |
| `src/evidence.rs` | Evidence status and expiry semantics |
| `src/assurance.rs` | Action contracts, decisions, and full recomputation |
| `tests/milestone_one.rs` | Validity, expiry boundary, blocking, and locality checks |
| `examples/expiry.rs` | Runnable permission-before-and-after-expiry example |

## Later milestones

1. Derived evidence, threshold rules, alternative witnesses, and support horizons.
2. Incremental evaluation checked against a full reference evaluator.
3. Evidence versions, assurance leases, and start/run/commit/outcome contracts.
4. A discrete-event building simulator, baseline policies, and fault experiments.
5. Distributed communication, an independently constrained AI planner, and
   eventually hardware adapters.

Those mechanisms are intentionally not implemented here. First understand and
validate this small rule before extending it.
