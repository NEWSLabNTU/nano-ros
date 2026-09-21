---
id: 1446
title: "`check-scaffold-builds` looks for a scaffolded Rust PROJECT's binary in
  two places RFC-0098 moved it out of, so the one variant that builds fine is
  the one it calls a vacuous pass"
status: open
type: bug
area: ci, tooling
severity: high
found: 2026-09-22
related: [rfc-0098, 1439, 1357, 1058, 1310, 1226, 1448]
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

## Why it costs more than one gate — merged from issue 1448

`just ci gate` stops at the first failing step, so a red here **withdraws**
`check::api-parity`, `test-unit` and `test-lane-contracts` on every branch that
runs the lane — the lane CLAUDE.md tells every contributor to run before every
push. That is issue 1226's shape: a lane with no signal capacity, where the
next regression looks exactly like yesterday's failure. It is also why a
contributor who reads `CI GATE FAILED` here cannot tell their own defect from
this one, which is how three separate sessions hit it inside a week and two of
them filed it.

MEASURED across two checkouts, two branches and two sessions on 2026-09-22,
each rebased onto `main`, which is the evidence that it is `main`'s and not any
branch's:

| checkout | branch | result |
|---|---|---|
| `nano-ros-box2` | `fix/1437-c-cpp-granted-qos` | `rust_project: FAIL`, same wording |
| `nano-ros-box-box` | `feat/444-name-accessors` | `rust_project: FAIL`, same wording |
| `nano-ros-box2` | `docs/1447-1448-dedup` (DOCS ONLY) | `rust_project: FAIL`, same wording; `check::build` 1 of 22 |

The third row settles it. That branch's entire diff against `main` is three
files under `docs/issues/`, not one line of code, and `check::build` still
fails — on this gate and on nothing else. A branch that changes only markdown
cannot break a build, so what it reproduces is `main`'s. The first two rows
also touch neither `packages/cli`, the `nros new` templates, nor anything a
scaffold compiles against except by ADDING inherent methods; and the root cause
above says why none of them could matter: no diff can move a file from depth 4
to depth 2.

## What this is NOT — merged from issue 1448

- **Not issue 1439.** That one is a RUNTIME failure of the scaffolded Rust
  entry — it builds, links, opens its session and exits without publishing.
  Here the gate's verdict is about the BUILD, and the build is fine, so 1439's
  subject is a different stage entirely.
- **Not a disk-space or load artifact.** The runs above were on different
  checkouts with different build state, and every other variant in the same
  invocation passed.
- **Not the flake beside it.** `a_stalled_timer_reports_a_timer_overrun_violation`
  also failed in the same sweep and passes solo (0.13 s); that is the known
  in-sweep timing flake, not this.
- **Not the `nros new` Rust PROJECT template.** Issue 1448 proposed starting
  there — at #1150 (issue 1409) and #1155 / `a0f00adad` (issue 1435), the two
  template changes of that week. That lead is SUPERSEDED by the root cause
  above: the template emits a project that compiles, and the locator is what
  cannot see the result. Recorded because a wrong lead that is merely deleted
  gets re-derived by the next reader.

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
