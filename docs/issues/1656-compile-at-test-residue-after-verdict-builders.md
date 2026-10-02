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
