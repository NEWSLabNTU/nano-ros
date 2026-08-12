---
id: 523
title: "Tier 1 red on four tests from phase-348: the compile-at-test detector cannot tell `cmake -P` from a configure, two new gates are genuinely unregistered, and three `lane_build_covers_run` cases time out at 60 s"
status: resolved
resolved_in: phase-348
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

### Fix — DONE 2026-08-12

1. `cmake_line_builds()` replaces the `"cmake -"` prefix. It reads the MODE:
   `-P` / `-E` / `--version` / `--help` are not builds; `-S` / `-B` / `--build`
   anywhere in the line are; otherwise it is only an invocation if the next
   token looks like one (a `-D` define, or a source directory).

   That last clause was not in the original plan and is the half that took two
   iterations. A "default to true so unknown spellings fail closed" rule
   promptly flagged two MORE compliant files, because the word `cmake` occurs in
   this tree outside any invocation:

   ```
   echo "cmake output was:" >&2                                  (prose)
   check "multi-part coord" … "$(nros_build_dir cmake workspace c)"   (an argument)
   ```

   Both are pinned in the self-test now, as is `cmake -E env FOO=1 cmake -S . -B build`
   — which the FIRST version of the predicate got wrong, because it read only
   the segment up to the next `cmake ` and so saw script mode. My own self-test
   caught that before it landed, which is the argument for writing one.

2. `provider_index_gate.sh` and `workspace_order_gate.sh` registered with
   `tool: "cmake (configure only)"`, matching `cargo_target_spelling.sh`'s
   precedent. Reasons are derived from each script's own header — what each
   asserts is the OUTPUT of a configure (returned provider rows and
   `CMAKE_CONFIGURE_DEPENDS` contents; an ordering that only exists once
   `ORDER_FROM_DEPENDS` has run), so a prebuilt artifact could only show the
   seam ran once, not that it resolves correctly. The phase-348 author should
   correct the wording if the intent differs; the rows are marked as derived.

Verified: `negative_diagnostic_registry` 3/3. Tripwired both ways — an
unregistered script running `cmake -S . -B build` is caught, and so is one
running `cargo build --release`.

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

### Cause — a `cargo run` on a 60-second clock

Nothing hangs. `lane_coords_file()` called `nros_lane_coords_file <lane>`, whose
body is:

```sh
cargo run -q -p nros-tests --bin lane-coords -- "$lane" > "$tmp"
```

`lane-coords` is a bin of `nros-tests`, so ANY edit to that crate invalidates it
and the next call recompiles the whole package before writing a byte. The shell
function's own comment says so — *"`cargo run` then COMPILES for
seconds-to-minutes"* — it just says it about a different reader. Three cases
call it; those three blew the 60 s per-test timeout.

**Which corrects this issue's original provenance line.** This was not phase-340
W3's env change. The trigger is "any `nros-tests` edit, then the first run" —
and the edits were mine (issue 0470 added `port_lease.rs` and touched
`large_msg.rs`). A test that compiles at runtime is a landmine for whoever edits
the crate next, which is CLAUDE.md's rule and archived issue 0041's whole point.

### Fix — DONE 2026-08-12, in TWO passes. The first was half a fix.

**Pass 1 (insufficient).** The test helper was changed to run the prebuilt
`lane-coords` instead of `cargo run`. Solo: 60 s timeouts → 9/9 in 0.5 s, and I
reported it fixed on that evidence.

It was not. The next full sweep timed out on the same three cases, because the
helper is not the only path to the compile: the GUARD UNDER TEST reaches it too.
`lane_sh` → `nros_fixtures_stamp_require` → `nros_lane_coords_file` →
`cargo run`. Solo that call is instant because nothing contends; inside a sweep,
concurrent cargos hold the package-cache and build-directory locks and it blocks
past 60 s. **A solo pass could not have distinguished a fixed path from an
unfixed one** — which is exactly the trap this issue is about, walked into while
fixing it.

**Pass 2 (the fix).** `nros_lane_coords_file` itself now prefers a prebuilt
selector, so every caller is covered — the three preflight call sites inside
this file included, which is where the guard reaches it:

```sh
bin="$(_nros_lane_coords_bin)"
if [ -n "$bin" ]; then "$bin" "$lane" > "$tmp"
elif ! cargo run -q -p nros-tests --bin lane-coords -- "$lane" > "$tmp"; then …
```

`_nros_lane_coords_bin` returns the NEWEST prebuilt selector, or empty — forcing
the `cargo run` rebuild — when any `nros-tests` source is newer than it. The
build recipes keep working either way; they just stop paying for a compile
somebody already did.

Verified in a full sweep, which is the only thing that can settle it:
**TIMEOUT count 3 → 0**, `lane_build_covers_run` absent from the failure list,
real failures 14 → 10. Plus: selector resolves to `target/debug/lane-coords`,
`tier2` still yields 13 coordinates, backdating every prebuilt binary makes it
correctly fall back to `cargo run`, `bash -n` clean.

The compile does not vanish, it MOVES: `cargo nextest run -p nros-tests` builds
that bin in its build phase before any test starts. Verified rather than assumed
— an artifact backdated to 2020 came back stamped today during the run. So the
test consumes a build-stage artifact, which is exactly the shape the rule asks
for.

**Selecting it by preferred profile was wrong and the tests caught it.** (This
applies to both passes; the shell helper picks the newest for the same reason.) The
first version tried `nros-fast-release` first and picked an ELEVEN-DAY-OLD
binary that answered `tier2` with 12 coordinates where the current sources say
13; two cases then failed with a coordinate-drift report that read like a bug in
the guard. Now: newest by mtime. A staleness check was written for this and then
REMOVED — nextest rebuilds the bin, so it could not fire in any supported
invocation, and unreachable code with a confident comment is the same defect
this issue is about.

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
**A is fixed (2026-08-12); this issue stays OPEN for B**, which is a separate
root cause and needs its own pass.

A.2's reasons were derived from each script's own header rather than left for
the phase-348 author: what both assert IS the output of a configure, which the
headers state plainly, so the rows could be written truthfully without guessing
at intent. They are marked as derived so that author can correct the wording.
