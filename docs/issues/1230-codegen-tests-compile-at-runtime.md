---
id: 1230
title: "four `rosidl-codegen` tests spawn `cargo check`/`cargo clippy` at TEST RUNTIME, against the no-compilation-inside-tests rule"
status: open
type: tech-debt
area: testing, codegen
severity: low
found: 2026-09-08
related: [1176]
---

# The record issue 1176 deliberately did not close

CLAUDE.md and AGENTS.md state it without qualification: **no compilation inside
tests** — never `cargo`/`cmake`/`idf.py`/`west build` at run time; compile in the
build stage (`build-test-fixtures` + `examples/fixtures.toml`) and let the test
consume the prebuilt artifact. "Does it compile?" intent becomes a build-step
fixture and an assertion about the artifact.

`packages/cli/rosidl-codegen/tests/compilation_test.rs` does not, in four tests:

| test | what it spawns |
| --- | --- |
| `test_simple_message_compiles` | `cargo check` |
| `test_message_with_arrays_compiles` | `cargo check` |
| `test_check_no_warnings` | `cargo check` |
| `test_clippy_no_warnings` | `cargo clippy` |

Each generates a crate into a `tempfile` tree from
`generate_message_package(…)` and then compiles it in place.

Issue 1176 found these while fixing a different defect in the same file and
recorded them rather than fixing them, "so the next reader of these files is not
surprised". This is that record, moved out of 1176 so it survives 1176's
archival.

## The adjacent pair 1176 also named, and how they differ

Checked, because the first draft of this issue got it wrong.

1176 also named `packages/rmw/cyclonedds/nros-rmw-cyclonedds/tests/
bare_metal_link.rs`. It does spawn a compiler — `bare_metal_no_std_clean` runs
`cargo build --target thumbv7m-none-eabi`, and `bare_metal_no_alloc_symbols`
runs `alloc_free_audit.sh`, whose line 43 is the same `cargo build` before the
`nm` scan. **But both carry `#[ignore]` with an explicit reason** (`"heavy:
invokes cargo build for thumbv7m-none-eabi"`), so neither runs in an ordinary
sweep: the compilation is opt-in and labelled as expensive at the call site.
That is a different situation from the four above, which run on every
`cargo test -p rosidl-codegen`, and it is why this issue is scoped to
`compilation_test.rs`. Whether an `#[ignore]`d heavy test that no lane names is
its own problem is a separate question and not this one.

## Why it is not a one-line fix

The obvious remedy — make it a fixture — does not apply cleanly. These tests
compile code **that does not exist until the test generates it**: the input is
`generate_message_package(…)` output for a message defined inline in the test
body. Moving that to the build stage means a `fixtures.toml` row whose producer
is the codegen library under test, in `packages/cli` — a sub-workspace
`build-test-fixtures` does not reach today. That is a phase, not a patch, and it
should be designed rather than improvised.

## Why it has not hurt yet, measured

The usual costs of runtime compilation are latency and a verdict that depends on
a toolchain the lane never provisioned. Measured 2026-09-08:
`cargo nextest run -p rosidl-codegen` is **10.7 s for 267 tests**, of which
`test_check_no_warnings` alone is **9.9 s** — so these four are most of the
suite's wall time and still cheap in absolute terms. And the toolchain question
is bounded here in a way it is not for an RTOS fixture: the test process was
spawned by cargo, so `cargo` is reachable by construction. `compilation_test.rs`
says exactly that in `require_cargo`, which is why issue 1160 turned its old
`if !cargo_available() { return }` into an assertion.

So this is real debt with a known blast radius, not a live defect. `severity:
low` on purpose.

## What would close it

Either:

1. a build-stage fixture for the generated-crate compile checks — which needs
   `build-test-fixtures` to reach `packages/cli`, and a coordinate for it; or
2. an explicit, documented exemption in AGENTS.md naming these four sites and
   the reason (input generated at test time; cargo reachable by construction),
   so the rule stops having four silent exceptions.

(2) is the honest cheap answer and should not be taken without first deciding
(1) is not worth it. What is not acceptable is the current state, where a stated
absolute rule has four unrecorded exceptions.
