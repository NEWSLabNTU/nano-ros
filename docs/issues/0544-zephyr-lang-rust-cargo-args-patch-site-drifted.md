---
id: 544
title: "`cargo-features-patch.sh` targets a line shape upstream no longer has, and the hand-patch that compensated duplicates the flag"
status: open
type: bug
area: zephyr
related: [issue-0432, phase-168]
---

## Symptom

Every Zephyr **Rust** fixture leaf fails at the `cargo build` step — all three
RMWs alike, so it takes the whole zephyr fixture module down and with it
`ci-matrix`:

```
error: the argument '--no-default-features' cannot be used multiple times
```

The C/C++ lanes are unaffected, which is why a C-only verification of an
unrelated fix (#534) passed cleanly against the same tree.

## Cause — one pass-through, injected in two places

`EXTRA_CARGO_ARGS` carries the per-example RMW feature set
(`--no-default-features --features rmw-<x>`) into zephyr-lang-rust's cargo
invocation. In the workspace's `modules/lang/rust/CMakeLists.txt` it appeared
**twice** on one command line: once right after `${rust_build_type_arg}`, once
at the end.

Upstream at the pinned `404fcef` has neither — both call sites are bare:

```console
$ git show HEAD:CMakeLists.txt | grep -n 'CARGO_ARGS'
220:    CARGO_ARGS build
239:    CARGO_ARGS doc
```

So all three occurrences were local, and they came from two different eras:

1. **`scripts/zephyr/cargo-features-patch.sh`** (tracked) injects
   `${EXTRA_CARGO_ARGS}` **inside** `add_cargo_target_with_zephyr_env`, after
   the line containing only `${rust_build_type_arg}`. This covers every caller.
2. **Two hand edits at the CALL SITES** — `CARGO_ARGS build ${EXTRA_CARGO_ARGS}`
   and `CARGO_ARGS doc ${EXTRA_CARGO_ARGS}` — carrying a nano-ros comment
   ("Phase 168.1") but present in **no tracked producer**. `git grep` for their
   text across the repo returns nothing.

Function-level injection plus caller-level injection = the flag twice.

**Why the hand edits exist.** The tracked script's own comment describes a
layout that is gone: *"Inject `${EXTRA_CARGO_ARGS}` immediately after every line
containing only `${rust_build_type_arg}`. There are two such lines: cargo build
(~199) and cargo doc (~243)."* Upstream has since refactored those two commands
into one shared `add_cargo_target_with_zephyr_env`, so the script now matches
**one** line, not two. Someone hit "XRCE/cyclone examples silently build the
default rmw-zenoh feature", patched the call sites by hand, and the script's
guard — `grep -q "nano-ros: EXTRA_CARGO_ARGS pass-through"`, which matches only
its OWN marker — could not see them.

**Why it surfaced only now.** A duplicated flag is a cargo CLI error, not a
build error, so it fails identically for every RMW and every leaf. Whether older
cargo tolerated the repetition was not established; what is certain is that
cargo 1.97.1 rejects it, and that the C lane never sees `EXTRA_CARGO_ARGS`.

## Worked around, NOT fixed

The two caller-level copies were removed from the local workspace
(`nano-ros-workspace/modules/lang/rust/CMakeLists.txt`), leaving the
function-level injection — which is the state a fresh provision produces.
Verified on two Rust leaves: `native_sim` (239 s) and `mps2_an385` (238 s),
both `ok`, zero duplicate-flag errors.

That is a repair of one machine's workspace. **The repo is unchanged and the
next provision reproduces the hazard**, because:

* the hand edits are in no tracked producer, so nothing re-creates them — but
  nothing removes them either, and any workspace that still carries them keeps
  failing;
* the tracked script still matches a line shape upstream refactored away, so its
  "two such lines" comment is wrong and its coverage is half what it claims.

## Direction

Deliver this the way the gpio fix is delivered (issue 0432): a real patch in
`zephyr/patches/` with a `module: zephyr-lang-rust` entry in `zephyr/patches.yml`,
sha256-verified by `west patch`, replacing the awk script that pattern-matches a
moving target. A patch that no longer applies fails loudly; an awk script whose
regex stops matching injects nothing and is silent — which is exactly how the
coverage halved without anyone noticing.

Whatever the mechanism, the invariant worth gating is that `EXTRA_CARGO_ARGS`
reaches the cargo command **exactly once**.
