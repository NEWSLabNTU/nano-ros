---
id: 1852
title: "`<target>/nros-c-generated` is still joined by hand ~140 times, outside `nros locate artifact` and out of reach of its gate"
status: open
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
