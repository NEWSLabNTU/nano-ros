---
id: 1230
title: "four `rosidl-codegen` tests spawn `cargo check`/`cargo clippy` at TEST RUNTIME, against the no-compilation-inside-tests rule"
status: resolved
type: tech-debt
area: testing, codegen
severity: low
found: 2026-09-08
resolved: 2026-09-09
related: [1176, 1244]
---

# Resolved: the four compiles moved to the build stage

`packages/cli/rosidl-codegen/tests/compilation_test.rs` is DELETED. Its four
tests (`test_simple_message_compiles`, `test_message_with_arrays_compiles`,
`test_check_no_warnings`, `test_clippy_no_warnings`) are now one
`[[compile_check_fixture]]` row and one consumer:

| piece | where |
| --- | --- |
| the four `.msg` inputs (were string constants in the test bodies) | `packages/testing/nros-tests/fixtures/generated_message_crate/msgs/*.msg` |
| the generation, at build time | that fixture's `build.rs`, path-depending on `rosidl-codegen` |
| the compile + lints | manifest row `generated_message_crate`, builder `cargo-clippy` |
| the assertion | `packages/testing/nros-tests/tests/generated_message_code_compiles.rs` |

## The "phase, not a patch" conclusion did not hold

This issue argued that a fixture row was blocked because the tests compile code
"that does not exist until the test generates it", so a row would need
`build-test-fixtures` to reach `packages/cli`, "a sub-workspace it does not
reach today". Both halves were checked and neither survived:

* **The inputs are constants.** All four `.msg` bodies were literals in the test
  source. Nothing about them needed a running test, so only the GENERATION had
  to move, not any test-time state.
* **The build stage already reaches `packages/cli`, three ways.** It runs the
  `nros` CLI while staging (`stage_tree` shells `nros sync`), it drives
  `nros codegen entry` for every cmake row, and its px4 leg runs
  `nros generate-px4-msgs` and then `cargo check`s the generated leaf — which is
  precisely "generate with the codegen, compile at the build stage". A
  cross-workspace path dep is equally routine: `rosidl-codegen` itself path-deps
  `packages/core/nros-serdes` and `nros-core`.

What was actually missing was one builder (`cargo-clippy`, ~20 lines beside
`stage_and_check` plus its name in two vocabularies) and a template crate. The
conversion is 1 manifest row + 1 fixture + 1 test.

The `build.rs` path dep on the emitter is load-bearing, not a convenience:
cargo writes dep-info naming every `rosidl-codegen` source into the staged
target dir, `compile-check-signature.sh` folds that measured closure into the
row's `.inputsig`, and an emitter edit therefore re-stales this fixture with no
hand-maintained list. Measured: editing `rosidl-codegen/src/generator/msg.rs`
flips `compile-check-stale.sh` to `stale`, reverting flips it back. A
`post_stage` hook shelling a prebuilt tool would have had no such edge — the
museum-artifact class of #182 / issue 1018.

## What the move changed, besides obeying the rule

* **`check-cli-tests` is a required check on every pull request** and paid
  10.7 s for `cargo nextest run -p rosidl-codegen` (267 tests), 9.9 s of it in
  `test_check_no_warnings` alone. Measured after: **263 tests in 0.44 s** — the
  four were 96 % of that suite's wall time. The compile did not vanish, it
  moved: ~36 s cold (50 s measured through the pooled script), once per
  `build-test-fixtures`, in one jobserver-parallel row of 42.
* **Both lint verdicts got stronger, and one of them was vacuous.** Each old
  test generated ONE message and carried its own attributes; the crate-level
  `#![deny(warnings)]` + `#![deny(clippy::all)]` now reach all four shapes. And
  `test_clippy_no_warnings` ran `-W clippy::all` and failed only on the
  substring `"error"` in stderr — every clippy WARNING, which is all that
  invocation can produce, passed.
* **The stronger verdict immediately found two things the old tests could not.**
  `packs/rust` emits `.clone()` on `Copy` arrays — four `clippy::clone_on_copy`
  sites in `ArrayMsg`, filed as **issue 1244** and allowed at the narrowest
  scope with that id (the old clippy test used a message with no array field).
  And every String-carrying message provokes `invalid_value` on the emitted
  `std::mem::zeroed()`, because the test's hand-written `rosidl_runtime_rs` stub
  types `String` as `std::string::String` while the real one is a `#[repr(C)]`
  pointer triple; that allow sits on the rmw layer with the reason. The old
  "no warnings" test generated `Point` — the one shape with no String field.

## Not in scope, and still true

The adjacent `#[ignore]`d compile-spawning tests this issue named
(`nros-rmw-cyclonedds/tests/bare_metal_link.rs`, and `heap_compile_check.rs` /
its C and C++ siblings in the same `rosidl-codegen` tests dir) are unchanged:
they carry `#[ignore]` with an explicit "heavy: invokes cargo build" reason, so
the compilation is opt-in and labelled at the call site. Whether a heavy test no
lane names is its own problem remains a separate question.
