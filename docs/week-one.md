# Understanding Week 1

We built the part of our system that answers:

> “Do we currently have enough usable evidence to support this conclusion, and why?”

Imagine a building with three sensors and a ventilation fan. We want the system
to confirm a hazard when at least two sensors report it. We also want it to stop
relying on old readings, explain its answers, and record changes.

Week 1 implements those checks in software. It does not connect to real sensors,
control the fan, or use AI yet. The examples provide simulated observations.

## 1. Evidence: information the system can use

An **evidence record** represents an observation about a particular condition.
For example: “Sensor S1 reports that the gas level is high.”

In our code, this record is an `EvidenceAtom`. Its fields answer ordinary questions:

| Field | What it tells us | Example |
| --- | --- | --- |
| `evidence_id` | Which piece of evidence is this? | `s1.gas_high` |
| `evidence_type` | What kind of evidence is it? | `sensor` |
| `predicate` | What condition are we checking? | `gas_high` |
| `source_id` | Where did the observation come from? | `s1` |
| `version` | Which update is this for that evidence ID? | `10` |
| `observed_at` | When was the observation made? | 0 seconds |
| `expires_at` | When does its valid support end? | 2 seconds |
| `status` | Does it support the condition? | `Valid` |
| `payload_hash` | Optional identifier derived from the original data | Not used in our demo |

The current kernel receives the status as input. It does not turn a raw gas
reading into a trustworthy claim by itself. That needs domain-specific rules
and sensor validation later. A payload hash alone does not prove a reading is true.

## 2. Why we have three statuses

`AssuranceStatus` is the one enum we use for these three possibilities:

| Status | Plain meaning |
| --- | --- |
| `Valid` | The available evidence supports the condition. |
| `Invalid` | Under our rules, the condition is not satisfied. |
| `Unknown` | We lack enough usable information to decide. |

A sensor reporting “gas is not high” may provide an `Invalid` status for the
condition `gas_high`. A missing or expired reading is `Unknown` instead.

**Valid does not mean safe.** If the condition is “a hazard exists,” a valid
result means the evidence supports the existence of a hazard.

## 3. Rules: how we combine evidence

The specification calls each rule a **justification**. It has inputs
(`premises`), a required count (`threshold`), and an output (`conclusion`).

| Rule | Meaning | Example |
| --- | --- | --- |
| AND | Every input is required | Hazard confirmed AND fan healthy |
| OR | At least one input is required | Either of two alternative sources supports a condition |
| 2-of-3 | Any two of three inputs are required | Two sensors support hazard confirmation |

For a 2-of-3 rule:

- Two valid sensors and one unknown sensor: **Valid**. We already have enough.
- One valid, one unknown and one invalid: **Unknown**. The unknown could make the difference.
- One valid and two invalid: **Invalid**. We cannot reach the required two.

The code can use any valid required count, not just two out of three.

## 4. The graph: connecting rules together

A **graph** is our collection of evidence and conclusions, connected by rules.
A **node** is one item in that graph.

```text
S1, S2, thermal sensor
        │
        │ at least two must support the hazard
        ▼
Hazard confirmed ───┐
                    │ both required
Fan healthy ────────┘
                    ▼
       Fan start conditions satisfied
```

The evaluator checks the sensor evidence before the hazard conclusion, then
checks the fan's start conditions. This dependency-first order is called
**topological order**.

We reject circular reasoning such as “A is supported by B, and B is supported
by A.” We also reject broken configurations, such as a rule naming a sensor
that was never declared or requiring four inputs when it only has three.

A declared sensor with no reading is different: the graph is valid, but that
sensor's evidence is unknown.

The fan-start result is only a conclusion about its configured requirements.
Week 1 does not execute the fan command.

## 5. Witness: the explanation behind a valid answer

A **witness** is the selected set of evidence supporting a conclusion.

Suppose all three sensors support the hazard, but only two are required. The
system might explain:

> “Hazard confirmed, supported by S1 and S2.”

If S2 becomes unknown, it can select S1 and the thermal sensor instead.
The conclusion stays valid because another supporting pair exists.

When several choices work, the system uses consistent selection rules so
repeating the same experiment gives the same explanation.

## 6. Horizon: when that support expires

A **horizon** is a deadline, not a duration measured from the current moment.

Suppose three valid observations expire at these simulated times:

| Sensor | Expiry time |
| --- | --- |
| S1 | 5 seconds |
| S2 | 8 seconds |
| Thermal | 12 seconds |

For a 2-of-3 rule, the system selects S2 and Thermal. That pair supports the
conclusion until **8 seconds**, when the first member of the pair expires.
At exactly 8 seconds, that pair is no longer usable and the conclusion must be checked again.

This is why we keep two related types:

- `AssuranceStatus`: just Valid, Unknown or Invalid.
- `AssuranceValue`: the result, including a horizon when it is Valid.

A later observation can change the answer before the horizon. The horizon is
not a promise that nothing will change or that a physical action is safe.

## 7. Time, versions and epochs are different things

| Term | What it tracks |
| --- | --- |
| Simulated time (`t`) | How far we have advanced the experiment's clock |
| Evidence version | Which observation update we have for one evidence ID |
| Epoch | How many evidence updates and expiry events the runtime has accepted or processed |

The program starts at time zero. We advance time explicitly; we do not wait
for real seconds to pass. This makes experiments repeatable.

Three sensor updates can all arrive at simulated time zero. We then have
`t=0`, but `epoch=3`. Time has not moved; three updates have happened.

An observation's version changes when a newer observation arrives. Its version
does not change just because it becomes too old to use.

## 8. What happens when evidence expires

Consider version 10 of an observation, received at time zero and expiring at 2 seconds:

```text
Time 0: accept version 10 → Valid, horizon 2 seconds, epoch 1
Time 2: process expiry    → Unknown, version still 10, epoch 2
Time 3: inspect result   → still Unknown; no new event
```

Even if we jump the simulated clock directly from 0 to 3 seconds, the runtime
processes the expiry at its deadline of 2 seconds.

It records an `EvidenceExpired` event and reevaluates the conclusions. It does
not need a new sensor message to notice that the existing evidence is stale.

A fresh observation can restore support. A replacement observation also replaces
the old deadline, so the old deadline cannot incorrectly expire the new reading.

## 9. The audit log: a diary of changes

The runtime keeps a history that explains what changed:

| Event | Meaning |
| --- | --- |
| `EvidenceUpdated` | We accepted a new observation. |
| `EvidenceExpired` | A valid observation reached its deadline. |
| `AssuranceChanged` | A conclusion's status or support deadline changed. |
| `WitnessChanged` | The evidence selected to explain a conclusion changed. |

Each entry includes simulated time and epoch. The history currently lives in
memory; it is not automatically saved to a database or file.

## 10. How the code fits together

You will usually interact with `AssuranceRuntime`, which coordinates the parts:

| Call | What it does |
| --- | --- |
| `AssuranceGraph::new(...)` | Defines and validates the evidence nodes and rules. |
| `AssuranceRuntime::new(graph)` | Starts an experiment with that graph. |
| `runtime.update(atom)` | Supplies an observation. |
| `runtime.advance_to(time)` | Moves simulated time forward and processes expiries. |
| `runtime.evaluation()` | Shows the current conclusions and their witnesses. |
| `runtime.audit()` | Shows the history of changes. |

In Week 1, every evidence update or expiry batch triggers a complete graph
check. That simple implementation is our **full reference evaluator**. Week 2
will add a more selective evaluator and check that it produces the same answers.

## 11. What our demonstration proves

Run this from the project directory:

```sh
cargo run --offline --example week_one
```

It demonstrates two scenarios from the specification:

- **C1 — One sensor becomes unknown:** a 2-of-3 hazard rule stays valid because
  the other two sensors still provide support. Its witness changes.
- **C2 — No new message arrives:** version 10 expires at 2 seconds. At 3 seconds
  it is unknown, while its version remains 10. The expiry is recorded at 2 seconds.

The automated tests also check rule combinations, invalid graphs, deadlines,
replacement observations, and whether unrelated action roots retain their results.
Run them with `cargo test --offline`.

These tests check our implementation's behavior. They do not establish that
real sensors are truthful or that the whole building is physically safe.

## 12. What comes next

Week 2 will independently check witness choices on small graphs, then add
**incremental evaluation**: revisiting only conclusions that could be affected
by a change. The full evaluator stays available to check its answers.

Action execution, permission leases, the building simulator and the Python AI
planner come later in the timeline.

For exact field contracts, selection rules, event ordering, implementation
assumptions and test mappings, see the [technical reference](week-one-reference.md).
The reference also records where we made explicit choices because the supplied
specification did not state every detail. Implementation Specification v1 remains
the technical authority for this work.
