---
id: 523
title: "Tier 1 red on four tests from phase-348: the compile-at-test detector cannot tell `cmake -P` from a configure, two new gates are genuinely unregistered, and three `lane_build_covers_run` cases time out at 60 s"
status: open
type: bug
severity: high
area: testing
related: [issue-0196, issue-0501, issue-0041, phase-348, phase-340]
---

## Symptom

`just ci` (tier 1), after the phase-348 W3/W4 commits landed:

```
Summary [101.279s] 1387 tests run: 1354 passed, 30 failed, 3 timed out, 73 skipped
Real failures: 4 / 4
  nros-tests::negative_diagnostic_registry :: enforce_registry
  nros-tests::lane_build_covers_run :: a_narrow_build_is_refused_when_the_run_is_not_narrowed
  nros-tests::lane_build_covers_run :: a_narrower_lane_build_still_does_not_satisfy_a_wider_run
  nros-tests::lane_build_covers_run :: a_tier2_build_satisfies_the_tier2_run_because_that_run_is_narrowed
```

Two independent problems. Neither is caused by the change that hit them (a
`scripts/` gate fix and four clippy lints in `provider_scan`'s tests).

---

## A. `enforce_registry` — one FALSE POSITIVE, two genuine gaps

```
unsanctioned compile-at-test — 3 file(s) invoke a compiler/build tool at RUNTIME
but are not in the negative-diagnostic registry (AGENTS.md E1 / issue 0196):
  package_xml_comment_stripping.sh
  provider_index_gate.sh
  workspace_order_gate.sh
```

They are not the same case, and lumping them is what makes the message
misleading:

| script | what it actually runs | verdict |
| --- | --- | --- |
| `package_xml_comment_stripping.sh` | `cmake -P run.cmake` | **false positive** |
| `provider_index_gate.sh` | `cmake -S . -B build` | genuine configure |
| `workspace_order_gate.sh` | `cmake -S . -B build_order` | genuine configure |

**The detector cannot tell script mode from a build.** `sh_invokes_build`
matches the needle `"cmake -"`, which hits `cmake -P` — CMake's *interpreter*
mode. It compiles nothing, configures nothing, and needs no fixture. The script
says so in its own header:

> Buildless: `cmake -P`, no compiler, no cargo, no fixtures.

So the gate is demanding a registry row (or a fixture-stage migration) for a
test that already complies with the rule the registry exists to enforce. That is
the shape issue 0196 warns about — a gate whose coverage does not match the rule
it enforces — and the cost is that the honest fix ("register it") launders a
non-violation into the registry, teaching the next reader that `cmake -P` is a
build.

The other two really do configure, and the registry already has the precedent
for exactly that: `cargo_target_spelling.sh` is registered with
`tool: "cmake (configure only)"`. They want rows with the same shape, or a move
to the fixture stage.

### Fix

1. Narrow the needle so `cmake -P` is not a build: match a configure
   (`cmake -S`, `cmake -B`, `cmake --build`, or a bare `cmake <dir>`) rather
   than the prefix `cmake -`. Add `cmake -P` to the gate's own self-test, since
   the failure mode is a plausible-looking red on a compliant file.
2. Register `provider_index_gate.sh` and `workspace_order_gate.sh` with
   `tool: "cmake (configure only)"` and a reason, or move them to the fixture
   stage.

---

## B. `lane_build_covers_run` — three cases time out at 60 s

All three shell out:

```rust
let mut cmd = Command::new("bash");
cmd.arg("-c").arg(format!("set -u; source scripts/build/fixture-lane.sh; {snippet}"))
```

`fixture-lane.sh` is sourced and the snippet exercises `nros_fixtures_stamp_require`.
Something in that path now blocks for over 60 s — the nextest per-test timeout —
where it previously returned. Three of the binary's cases hit it; the others do
not, so it is a specific arm rather than the whole file.

Not yet root-caused. Worth checking first, because the file's own header
describes exactly this hazard: phase-340 W3 made the guard also require a
narrowed RUN, and the helper deliberately CLEARS `NROS_TEST_COORDS`,
`NROS_FIXTURE_LANE`, `NROS_FIXTURE_STAMP` so the child cannot inherit them. A
guard that waits on something (a lock, a stamp probe, a `just` recipe) instead
of refusing will hang exactly when those are unset.

This is also the compile-at-test class one layer out: a test whose subject is a
shell function that can invoke the build system has no bounded runtime, which is
why 60 s is both arbitrary and reachable. Issue 0501 was the same shape in
`native_main_macro_misuse`, and archived 0041 was it suite-wide.

---

## Reproduce

```sh
source ./activate.sh
cargo nextest run -p nros-tests --test negative_diagnostic_registry
cargo nextest run -p nros-tests --test lane_build_covers_run
```

## Provenance

`negative_diagnostic_registry`'s three files arrived with phase-348 W3/W4
(`af70ca723`, `2efa47430`); `lane_build_covers_run` last moved in
`fdec5824f test(phase-340 W3): the lane guard's child must not inherit the lane env`.
Filed rather than fixed: A.2 needs the phase-348 author's decision on WHY each
gate must configure at runtime (the registry's `reason` field is the point of
it, and guessing defeats the gate), and B needs its own root-cause pass.
