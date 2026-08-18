---
id: 676
title: "The `--locked` shim treats `--offline` as already-pinned, so any offline cargo command may silently rewrite Cargo.lock"
status: resolved
type: bug
area: build
related: [issue-0359, issue-0378, issue-0648]
---

## The defect

`scripts/bin/cargo` injects `--locked` project-wide (CLAUDE.md: "Lockfiles
change ONLY when a dev means it"). It skipped injection whenever the caller had
already passed a lock-ish flag:

```sh
for a in "$@"; do
    case "$a" in
        --locked | --frozen | --offline)   # <- --offline does not belong here
            exec "$real_cargo" "$@"
```

`--frozen` genuinely means `--locked --offline`, so skipping for it is right.
**`--offline` does not imply `--locked`.** It only restricts cargo to the local
cache; resolution still runs, and cargo will rewrite `Cargo.lock` from whatever
the cache happens to hold.

## How it surfaced

Not from a build — from the measurement run for
[issue 0648](archived/0648-cargo-package-cache-lock-serialises-the-fanout.md),
whose whole point was an `--offline` arm. It moved `libc` in two leaf locks:

```
-version = "0.2.183"
+version = "0.2.189"
+source = "registry+https://github.com/rust-lang/crates.io-index"
```

Two files, six lines, in a run that was supposed to be read-only. Reverted.
Nothing but `git status` would have reported it, which is the hazard: the guard
that exists to make this loud was the thing standing down.

## Confirmed by controlled repro

Same command, same leaf, before the fix:

| invocation | `Cargo.lock` moved? |
| --- | --- |
| `cargo metadata --manifest-path <leaf> --offline` | **YES** |
| `cargo metadata --manifest-path <leaf>` | no |

After the fix, the `--offline` arm no longer moves it, and `--offline` still
works (the shim now passes `--offline --locked`, which is valid and strictly
safer).

## Blast radius

Any `--offline` cargo invocation anywhere in the tree — recipes, scripts, ad-hoc
commands. `--offline` is recommended in several places precisely because it is
assumed inert, which made this the worst possible member of the skip list.

## Fix

Drop `--offline` from the skip list; keep `--locked` and `--frozen`. One line,
plus the comment explaining why the asymmetry is deliberate so it is not
"tidied" back.
