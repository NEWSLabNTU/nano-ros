---
id: 1446
title: "`check-scaffold-builds` looks for a scaffolded Rust PROJECT's binary in
  two places RFC-0098 moved it out of, so the one variant that builds fine is
  the one it calls a vacuous pass"
status: open
type: bug
area: ci, tooling
found: 2026-09-22
related: [rfc-0098, 1439, 1357]
---

## Problem

`just check scaffold-builds` reports

```
  rust_component: OK — 1 own artifact(s), e.g. target/debug/librust_component.rlib
  rust_project:   FAIL — the build exited 0 but compiled none of the emitted source
  c_component:    OK — 5 own artifact(s)
  c_project:      OK — 4 own artifact(s)
  cpp_component:  OK — 2 own artifact(s)
  cpp_project:    OK — 4 own artifact(s)
```

and the `rust_project` build log ends `Compiling rust_project v0.1.0
(/tmp/nros-scaffold-builds.xhNcLF/rust_project)` / `Finished dev profile`. The
binary is there:

```
/tmp/nros-scaffold-builds.xhNcLF/rust_project/build/native/target/debug/rust_project
```

`own_artifacts()` in `scripts/check-scaffold-builds.sh` looks in exactly two
places for a cargo artifact:

```sh
find "$dir/build"  -maxdepth 2 \( -name "lib${name}*.a" -o -name "$name" \) -not -path '*CMakeFiles*'
find "$dir/target" -maxdepth 3 \( -name "$name" -o -name "lib${name}.rlib" \) -type f
```

`$dir/target` does not exist for this variant, and the binary sits FOUR levels
under `$dir/build`, so neither find can reach it at any depth the script allows.
`rust_component` passes because a component leaf still writes `target/debug/`;
the PROJECT variant writes `build/<image>/target/`, which is where RFC-0098 D1
(phase-445 W6) put a leaf's generated cargo output.

## Why the verdict is worse than a plain red

The message says "compiled none of the emitted source", which is a statement
about the BUILD, and the build is fine. A reader lands on the scaffold or on
`nros new` and finds nothing wrong with either. It also makes the gate's own
anti-vacuity argument backwards: the predicate exists to refuse "an artifact
exists somewhere" as a pass, and here it refuses the one artifact that IS the
emitted source.

## Not caused by the change that found it

Found while re-running `check build` under issue 1434. That change touches board
crates, nros-node, the nros-c/nros-cpp headers, the entry emitters and their
goldens — no cmake, no build script, no path logic — and no content of it can
move a file from depth 4 to depth 2. It failed identically on both `ci gate`
runs, before and after an unrelated CLI rebuild, and in both provisioning states
of the worktree.

## Fix shape

Widen the cargo arm to the RFC-0098 layout — `$dir/build/*/target/{debug,release}`
— rather than raising `-maxdepth`, which would readmit the `CMakeFiles` vacuity
the comment above it describes. Assert the two layouts by NAME so the next move
is a failing test rather than a silent miss, and check `rust_component` still
resolves through the old one.
