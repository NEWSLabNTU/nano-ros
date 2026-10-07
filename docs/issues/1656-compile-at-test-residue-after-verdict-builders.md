---
id: 1656
title: "Compile-at-test residue after the verdict builders: five registry
  entries, a cxx-syntax verdict builder, and two signature gaps"
status: open
type: tech-debt
area: testing, build
severity: low
found: 2026-10-03
related: [0034, 0196, 0501, 1230, 1620]
---

## Context

Issue 1620 added two VERDICT builders to the compile-check lane
(`cargo-check-verdict`, `cmake-configure-verdict`): the build stage runs a
compile, records its exit status and stderr, and succeeds whatever the compiler
said; the test asserts the verdict through
`nros_tests::fixtures::require_compile_verdict`. That retired the premise behind
"a must-fail compile is the one exception to *no compilation inside tests*", and
four files moved (`native_main_macro_misuse`, `cmake_platform_matrix`,
`cmake_node_register_misuse`, `diagnostic_verbatim`; `native_orchestration_misuse`
was deleted as a duplicate of `orch_tiers_multi`).

What is left is recorded here, rather than in 1620, because none of it was
part of 1620's acceptance and each needs its own decision.

## Still compiling at test time (`negative_diagnostic_registry.rs`)

Sweep: `git grep -n 'Command::new("cargo")\|Command::new("cmake")\|"west"' packages/testing/nros-tests/tests/`
plus `grep -l 'cargo build\|cargo check\|cmake -S\|cmake -B\|cmake --build' packages/testing/nros-tests/tests/*.sh`.

| file | why it is not a verdict row yet |
| --- | --- |
| `platform_header_compile.rs` | its one NEGATIVE cell (bare-metal heap without malloc must fail) is a plain must-fail compile — the shape the verdict builders exist for — but it is a `c++ -fsyntax-only` snippet, and only cargo/cmake verdict builders exist. A `cxx-syntax-verdict` builder is the fix. |
| `zpico_drift_gate.rs` | the corrupted half could be a verdict row; the corruption is a per-run sandbox of the CANONICAL platform tree injected through `NROS_PLATFORMS_DIR`, which a row's `dir` staging does not model. |
| `cross_libc_precedence_gate.rs` | RELATIVE: a broken and a fixed cross compile only mean something together, and the raw cross-g++ object compile maps to no builder. |
| `size_probe_verify.sh` | a determinism soak — the repeated clean build IS the assertion. |
| `provider_index_gate.sh`, `workspace_order_gate.sh`, `cargo_target_spelling.sh` | configure-only seam probes whose assertion is the configure's OUTPUT across several synthetic scopes; a single verdict cannot stand in. |
| `integration_px4.rs` | `make --just-print` — compiles nothing. |

Not compiles, listed so the sweep is complete: `zpico_build_matrix.rs`
(`cargo tree`), `bringup_scaffold.rs` / `exec_depend_drift_check.rs` /
`workspace_lints_check.rs` (`nros check`), `orchestration_e2e.rs` (`nros plan`),
`fvp_smoke.rs` / `fvp_runtime_ws.rs` (`just zephyr run-fvp-*` on a prebuilt ELF).

## Two signature gaps the verdict rows inherit

* **`post_stage` overlay text is not an input.** The per-id `main.rs` /
  `system.toml` rewrites live in `scripts/build/compile-check-fixtures.sh`,
  which no row's `.inputsig` hashes — so editing an overlay leaves the row
  reading FRESH. Pre-existing for the `main_macro_form*` rows; the five
  `main_macro_*` verdict rows have it too. The cmake verdict rows avoid it by
  keeping their cases as files (`fixtures/cmake_node_register_misuse/cases/`),
  which is the pattern to move the cargo overlays to.
* **A failing crate writes no dep-info.** For a `cargo-check-verdict` row whose
  compile fails, rustc writes no `.d` for the failing crate itself; its deps'
  dep-info (including `nros-macros`, which produces the diagnostics) is still
  read, so the closure is narrower than a passing row's by exactly the failing
  crate's own sources — which are the row's `dir`, already hashed.

## Cost

Each cargo verdict row stages its own target dir (~475 MB, ~15 s cold with
sccache), as the `main_macro_form*` rows already do. Issue 0501 is why they do
not share one; a shared dir with per-copy package versions would bring the six
rows down to one dependency build.

## `check-export-f-closure` scans `scripts/build/*.sh` only

Issue 1620 widened it to indented definitions (the esp32 packer's leaf functions
sit inside an `if`). The `export -f` lists in `justfile`, `just/native.just` and
`scripts/debug/debug-keyexpr.sh` are still outside its reach; each is closed by
hand today, which is the state the gate exists to stop relying on.

## `borrowed_e2e`'s stamp is built by its own gate recipe only

`build/borrowed-e2e/.compile-ok` is produced by `scripts/build/borrowed-e2e-fixture.sh`,
which only `just check borrowed-e2e` runs — no fixture lane builds it. Routing the
test through the shared absence funnel (as 1620 did for its siblings) would make
`check-lane-contracts` refuse it (the funnel is a RUNTIME resolver and `ci gate`
reaches the test) and would make a gated `test-all` fail on a stamp nothing in
that lane produces. It keeps its own existence probe, counted in
`.config/fixture-existence-skip-baseline.txt`. The fix is a `[[compile_check_fixture]]`
row for it, so the stamp has a lane.

## 2026-10-03 — the verdict targets left the gate lane

Measured by the phase-475 census after #1610 landed: the four converted targets
(`cmake_node_register_misuse`, `cmake_platform_matrix`, `diagnostic_verbatim`,
`native_main_macro_misuse`) now resolve a compile-check verdict stamp, which
the census does not stage, so they classify FIXTURE and are no longer admitted
to `test-lane-contracts`. Before #1610 they ran there (compiling at test time).
The coverage is not lost — the verdict rows build in `check-source-gates`' stamp
build and the tests run wherever stamps exist — but no merge-gating lane runs
the TESTS now. Remedy: have `test-lane-contracts` build the stamps for the
verdict rows its admitted targets read (a compile-stage stamp is allowed there
by `check-lane-contracts`), and have the census stage the same set.

## 2026-10-07 — four of five sections closed; `export -f` reach still open

### The verdict targets are back in the gate lane

`test-lane-contracts` now builds the compile-check stamps its admitted tests
read, and only those: `scripts/test/lane-compile-stamps.py --admission
.config/lane-admission/gate.txt` derives the set from the tests' own source
(the ids a test passes to `require_compile_check` / `require_compile_verdict`,
restricted to `STAMP_COMPILE_CHECK_BUILDERS` in `fixtures-manifest.py` —
builders whose whole artifact is a stamp or verdict; a row that links a binary
a test runs is a fixture however it is reached), and the recipe hands it to
`compile-check-fixtures.sh` as `NROS_FIXTURE_IDS` (new: an id SET, validated,
fanned out through the pool). The census (`lane-census-run.sh`) stages the
`--census` superset — every compile-resolver target's stamp rows — serially
and keep-going, after building `nros-launch-resolve` as the gate job does.
One derivation, two callers, and `check-lane-contracts` calls the same
function to re-derive the set.

`check-lane-contracts` changed in two ways. The union of
`NROS_COMPILE_CHECK_LANES` values could not express an id-set invocation, so
coverage is now per invocation (lane filter × builder filter × id set). And
the producer must be the recipe that RUNS the test or one it depends on: the
closure-wide search called `platform_header_compile`'s new verdict row covered
by `test-lane-contracts`, a LATER step of `ci gate` than the
`check::source-gates` that runs it — measured, the gate reported OK; with the
rule it named both tests, and `source-gates` now asks for
`cxx-syntax,cxx-syntax-verdict,cxx-compile-verdict`. (`_lane_on` also took
only space-separated lanes while the gate reads only comma-separated ones —
the one spelling the gate could check selected nothing.) Selftest: 74 cases,
five new.

Regenerated in `nros-ci-local:humble-zenoh` (`--runs 2`): 56 whole targets +
31 single tests. New whole: the four (`cmake_node_register_misuse`,
`cmake_platform_matrix`, `diagnostic_verbatim`, `native_main_macro_misuse`)
plus `native_main_macro_forms`, `cpp_api_drift`, `generated_message_code_compiles`,
`macro_one_dep_resolves`, `platform_header_compile`, `zpico_drift_gate`. The
admitted set reads all 38 census rows.

**Cost**, cold (fresh build root, no sccache), 24 cores: 33 s wall / 265
CPU-s, 2.2 GB; warm re-run 18 s. In the census container, serial, 138 s.
`just test-lane-contracts` end to end with warm stamps: 51 s, 421 tests. The
per-row target dirs would have been ~730 MB per `n9_workspace` row (measured
4.9 GB for the 14 verdict rows alone): staged rows of one template now share a
target dir (`<compile-check>/.shared-target/<dir>`), with every workspace
member's version stamped `+<row>` — issue 0501's fix, so a sibling's
successful check can never satisfy a misuse row. Rows whose overlay changes an
EXTERNAL crate's input (the zpico drift rows) opt out with
`.private-target-dir`. The dep-info a shared dir holds is copied into each row
dir (`shared-target.d`, a union — wider, never narrower) so `.inputsig` keeps
its measured closure. On a 4-vCPU runner expect roughly 1–2 min cold for the
stamp step; judged affordable against the compiles these targets ran at test
time before #1610.

### Compiles that were still in tests

* `platform_header_compile` — the #38 negative cell is a `cxx-syntax-verdict`
  row (`platform_hdr_baremetal_heap_no_malloc`): the same compile and include
  set as `cxx-syntax`, recorded. The test now also asserts it failed ON the
  missing allocator.
* `zpico_drift_gate` — two `cargo-check-verdict` rows over
  `fixtures/zpico_drift_gate/`, which shadows only the posix descriptor
  through `NROS_PLATFORMS_DIR` (its `.cargo/config.toml` `[env]`; a
  descriptor is keyed by directory, the env root is searched first). The
  sandbox the test built per run is two frozen copies — canonical, and with
  the sentinel substitution — and a source-only test fails if either drifts
  from `packages/platform/nros-platform-posix/`. The assertion is tighter
  than before: the diagnostic must name the sentinel (it does; it fails in
  `cc1` on the nonexistent source, not in a "drift" message).
* `cross_libc_precedence_gate` — "a single verdict cannot express a RELATIVE
  check" was true and beside the point: it is three verdicts the TEST relates.
  `cxx-compile-verdict` rows (argument lists as files, `cases/<id>.args`;
  compiler `<target>-g++` from the SDK store then PATH, recorded in
  `verdict.tool`, `absent` when none — the test skips on that, as before).
* `size_probe_verify.sh`, `provider_index_gate.sh`, `workspace_order_gate.sh`,
  `cargo_target_spelling.sh` — NOT moved, and not tests: no nextest target
  runs them; each is the body of a `just` gate recipe (`verify-size-probe`,
  `check provider-index`, `check workspace-order`,
  `check cargo-target-spelling`), i.e. they already ARE the build stage. What
  they assert is a build's behaviour across repetitions (the determinism soak)
  or a configure's output across several synthetic scopes, which no single
  recorded verdict carries. They sit in `tests/` for history; the registry
  keeps them as `RuntimeException` rows with those reasons.
* `integration_px4.rs` — `make --just-print`, compiles nothing; unchanged.

`negative_diagnostic_registry` has no FAIL-path row left, and
`registry_well_formed` now refuses a new one (it used to REQUIRE one).
Sweep for the class: `git grep -n 'Command::new("cargo")\|Command::new("cmake")\|Command::new("g++")\|Command::new(gxx)' packages/testing/nros-tests/tests/`
— remaining hits are `zpico_build_matrix` (`cargo tree`) and the registry's
own needle list.

### `borrowed_e2e` has a row

`[[compile_check_fixture]] borrowed_e2e`, builder `fixture-script`: the
recipe is `<dir>/build.sh <row-dir>` (moved from
`scripts/build/borrowed-e2e-fixture.sh`, with both drivers, into
`fixtures/borrowed-e2e/`), so `.inputsig` hashes the script and drivers, and
the script copies the compilers' `-MD` output and cargo's dep-info for nros-c,
the FFI crate and `rosidl-codegen` into the row dir (closure measured: ~250
repo files). Every fixture lane builds it through `build-compile-check-fixtures`;
the test resolves it with `require_compile_check`, and an absent host compiler
is a recorded `<bin>.skipped`, not a missing file. Its existence-probe
baseline entry is gone. Measured: editing `c/driver.c` makes the row STALE.

### Overlay edits make rows stale

The `post_stage` `printf`/`sed` overlays are files under the template's
`cases/<id>/`, stored `<path>.case` (so by-name gates — board vocabulary,
deploy-board resolution, provider announcements — do not read a deliberately
wrong `system.toml` or descriptor as a real one) and staged as `<path>`. They
are inside `dir`, so `.inputsig` covers them. Measured: appending a line to
`cases/main_macro_form1/.../main.rs.case` makes `main_macro_form1` (and, the
dir hash being per template, its siblings) STALE. Two source-only tests keep
the `unknown_board` and `orch_tiers_single` overlays equal to their template
but for the one change. The `orch_tiers_single` model-strip was dead (no
template carries `config/system_model.yaml` since phase-330 W4.a) and went.

### Still open

* **`check-export-f-closure` reach** (the section above): the `export -f`
  lists in `justfile`, `just/native.just` and `scripts/debug/debug-keyexpr.sh`
  are still outside it. Not attempted here — those lists export
  recipe-local functions (`build_one`, `check_one`, `run_talker`) whose callees
  are not `nros_*`, which the walker does not follow, so widening the file set
  alone would report OK over a closure it cannot see.
* **A failing crate writes no dep-info** — inherent, as written above; the gap
  is the row's own `dir`, which is hashed.
