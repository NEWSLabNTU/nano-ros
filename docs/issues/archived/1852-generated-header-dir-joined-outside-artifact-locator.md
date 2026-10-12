---
id: 1852
title: "`<target>/nros-c-generated` is still joined by hand ~140 times, outside `nros locate artifact` and out of reach of its gate"
status: resolved
resolved: 2026-10-13
type: tech-debt
area: build
severity: low
found: 2026-10-13
related: [0088, 0268, 1767, RFC-0103, phase-484]
---

## Measured (2026-10-13)

Residue of issue 1767 item 12. Phase-484 W6a folded the three `OUT_DIR` JSON
parsers into `scripts/lib/cargo_out_dir.py`, and W6b made the layout under a
cargo target dir ONE function — `nros_build_paths::artifact::Artifact`
(`C_GENERATED_DIR = "nros-c-generated"`,
`packages/tooling/nros-build-paths/src/artifact.rs:22`), exposed as `nros locate
artifact`, cmake `nros_locate_artifact()` and shell `nros_locate_artifact`. The
PX4 `${NANO_ROS_ROOT}/target/...` literal is gone. Item 12's four arms are
therefore down to one owner on paper.

But the layout is still restated by hand where the target dir is a VARIABLE:

- `just/check/lanes.just` — 88 `nros-c-generated` and 56 `nros-cpp-generated`
  occurrences (`-I"$gen/nros-c-generated"` with `gen="$(nros_scoped_target_dir
  check-c)"`, `:94`; `check-cpp` at `:1106`).
- `cmake/NanoRosNodeRegister.cmake:713,952,1336-1337` and
  `cmake/NanoRosVerbs.cmake:527-528` — `${_gen_dir}/nros-c-generated/nros/
  nros_config_generated.h` under `_nros_generated_header_dir()`'s mirror root.
- `scripts/build/cargo-out-dir-headers.py:149-170` — the same suffix under an
  `OUT_DIR`.

`check-artifact-path-literals` keys on `target/` + an artifact name, so a join
onto `$gen` or `${_gen_dir}` is invisible to it (issue 0196's reach shape). The
answers agree today because every site spells the same constant; a rename of
`C_GENERATED_DIR` would move the writers (nros-c/nros-cpp build scripts read it)
and leave these readers on the old name.

## Direction

`lanes.just` asks `nros_locate_artifact c-include` once per recipe (the
target dir is its `CARGO_TARGET_DIR`, which `nros locate artifact` already
honours); the cmake mirror sites take the suffix from one cmake helper; the
gate learns the bare `nros-c(pp)-generated` component, not only `target/…`.

## Fix (2026-10-13)

The names have ONE definition, `nros_build_paths::artifact::{C,CPP}_GENERATED_DIR`.
Every road now asks for it or reads it, and none restates it:

- **A cargo target dir:** asked through `nros locate artifact`. In
  `just/check/lanes.just` the `c` and `cpp` lanes call
  `cgen="$(nros_locate_artifact c-include)"` and `cppgen=…` once, after
  exporting their own `CARGO_TARGET_DIR`. That replaces 136 hand joins
  (82 C, 54 C++), plus the param-services build's
  `-I"$param_dir/nros-c-generated"`. `size_probe_verify.sh` asks for
  `c-config-header`.
- **A mirror or include root,** which `nros locate artifact` cannot answer
  because it only knows cargo's target dir: handled by the new
  `cmake/NanoRosArtifactLayout.cmake`. Its `nros_generated_dir(<out> c|cpp)` and
  `nros_generated_header(<out> <root> c|cpp <file>)` READ the constant out of
  `artifact.rs`, so a rename moves them.
  - Routed through it: `NanoRosNodeRegister.cmake` (3 sites),
    `NanoRosVerbs.cmake`, `zephyr/CMakeLists.txt` (3 include dirs),
    `zephyr/cmake/nros_cargo_build.cmake` (5 byproducts), and the `nros-c` /
    `nros-cpp` `CMakeLists.txt` mirror and heal arguments.
  - The Zephyr and API sites were not in the original count above; the sweep
    found them.
- **Rust:** the `nros-c` / `nros-cpp` build scripts publish
  `cargo:config_include` with `nros_build_helpers::{C,CPP}_GENERATED_DIR`,
  re-exported from `artifact`.
- **Python:** the new `scripts/lib/artifact_layout.py` reads the same
  constants.
  - `cargo-out-dir-headers.py` recognised the directories by a `nros-*-generated`
    PATTERN, a third spelling; it now tests membership in `GEN_DIRS`.
  - `check-orphan-generated-stamp.py` had defined `GEN_DIRS` twice; it now
    imports it.
- **Gate:** `check-artifact-path-literals` refuses the bare
  `nros-c(pp)-generated` name in cmake, `just`, shell, make and Rust code,
  under any root. The `target/` rule still covers Python, where the remaining
  hits are docstrings and self-test fixtures.

## Verified

- Negative control: the gate's matcher flags 158 sites across the ten swept
  files at `origin/main`, and 0 after.
- `nros locate artifact c-include` under `CARGO_TARGET_DIR=<d>` prints
  `<d>/nros-c-generated` (70 ms), the value `lanes.just` used to build by hand.
- The cmake module returns both names, including when called from inside a
  function.
- `cargo-out-dir-headers.py --self-test` and `check-orphan-generated-stamp.py`
  pass.
- `just ci gate` runs the `c`/`cpp` lanes that `lanes.just` drives.
