---
id: 314
title: "Three Zephyr example recipes had empty loop bodies: build-c reported success while building nothing, build-cpp/build-xrce were bash syntax errors"
status: resolved
type: bug
area: build
related: [issue-0313]
---

## Finding (2026-07-28)

`just zephyr build-examples` could not have worked for a long time. It depends
on four recipes and **all four** were broken, in three different ways:

| recipe | state | symptom |
| --- | --- | --- |
| `build-c` | loop body deleted, loop removed too | **exit 0, builds nothing** |
| `build-cpp` | `for ... do` immediately followed by `done` | bash syntax error, exit 2 |
| `build-xrce` | two such empty loops | bash syntax error, exit 2 |
| `build-rust-examples` | loop intact | fails: `no matching package named zephyr-build` |

`build-c` is the dangerous one. The others fail loudly; `build-c` prints

```
Building Zephyr C examples in zephyr-workspace...
Zephyr C examples built successfully!
```

and exits 0 having compiled nothing. Everything between the banner and the
success line was gone, leaving only the leftover `NROS="$(basename "$(pwd)")"`
and `cd "$WORKSPACE"` scaffolding.

This is the CLAUDE.md rule *"Tests must fail on unmet preconditions — bare
`eprintln!` + `return` reports PASS, never"* in recipe form. A recipe that
claims success without doing the work is worse than one that fails, because it
launders a green result.

It was found the honest way: while verifying issue 0316 I used
`just zephyr build-c` as a receipt, got rc=0 in about two seconds, and the
speed was implausible for six Zephyr images. The first "green receipt" for that
fix was therefore vacuous.

## Cause

The empty loops predate the phase-221 track A rewrite (`09dcd2620`,
2026-06-04), which carried them forward rather than introducing them — the
`for ex in ...; do` / `done` skeletons appear in `just/zephyr.just` before that
commit too. So the bodies were deleted earlier and the skeletons left behind,
and nothing noticed because `build-examples` is not part of `just ci`.

`build-xrce`'s second loop was annotated `# Phase 95.C — cpp/xrce 6-example
set`, and `examples/zephyr/cpp/xrce/` does not exist.

### The abandoned directories that made it look plausible

nano-ros does not keep a per-RMW directory level. The path is
`<platform>/<language>/<example>`, and the backend is chosen at build/test
time — phase-168 collapsed the old `rust/{zenoh,xrce,dds}/` trees for exactly
this reason.

But three of those trees were still sitting on disk under
`examples/zephyr/rust/`, holding **735 files between them and zero tracked
ones** — pure `generated/` output left behind when the examples were removed.
Every file was under `generated/`; there was no source at all. They read like
real example trees to anything that only looked at directory names, which is
how a `cpp/xrce` loop survived review and how this investigation misread the
layout twice.

Removed: `examples/zephyr/rust/{xrce,zenoh,dds}`. The surviving
`zephyr/{rust,cpp}/cyclonedds/talker-aemv8r` is *not* an RMW directory — it is
a board variant (aemv8r) that happens to be nested under a backend name, and it
is tracked.

The convention is now stated explicitly in `examples/README.md` under "Tree
shape", so the next recipe author does not have to infer it.

## Fix

Restore the three loops, delegating to `build-one` exactly as the one intact
recipe (`build-rust-examples`) does. `build-one` owns the west invocation and
workspace setup and must run from the repo root, so the leftover
`cd "$WORKSPACE"` is dropped rather than preserved.

- `build-c` → `c/<role>` × 6, zenoh
- `build-cpp` → `cpp/<role>` × 6, zenoh
- `build-xrce` → `c/<role>` and `cpp/<role>` × 6 each, xrce

### Receipts

| recipe | before | after |
| --- | --- | --- |
| `build-c` | rc=0, **0 ELFs** | rc=0, **6 ELFs** |
| `build-cpp` | rc=2, syntax error | rc=0, **6 ELFs** |
| `build-xrce` | rc=2, syntax error | rc=0, **12 ELFs** |

Each ELF confirmed by a distinct `Built: .../build-<lang>-<role>-<rmw>/zephyr/zephyr.elf`
line, not by exit status alone — exit status is precisely what was lying.

## Still open: `build-rust-examples`

Not fixed here, because it is a different defect with a different cause. Every
`examples/zephyr/rust/*` build fails at metadata resolution:

```
error: no matching package named `zephyr-build` found
location searched: crates.io index
required by package `nros_zephyr_talker v0.1.0`
```

Confirmed under **both** backends (`rust/talker zenoh` and `rust/talker xrce`),
so it is unrelated to backend selection. `build-examples` therefore still fails
at its first dependency until this is resolved. Related history:
`archived/0211-zephyr-rust-buildrs-duplication.md`.

## Prevention

The gap that let this live: `build-examples` is not in `just ci`, and no test
asserts that a build recipe produced an artifact. The cheap guard is the one
used above — assert the expected ELF count rather than trusting rc=0. Worth
considering as a follow-up; recipes that can silently succeed are the same
hazard class as the count-based proofs retired in issue 0309.
