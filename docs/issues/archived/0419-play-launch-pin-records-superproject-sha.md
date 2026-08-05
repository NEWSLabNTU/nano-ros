---
id: 419
title: "The play_launch pin records the SUPERPROJECT sha when the submodule is uninitialised, and never re-stamps once it is"
status: resolved
type: bug
area: build
related: [issue-0409, phase-338]
---

## Symptom

On a checkout where `packages/cli/third-party/play_launch` has not been
initialised, `nros sync` fails the issue-0409 resolver-pin guard with a message
that cannot be true:

```
Error: sync: `…/nros-launch-resolve` was built from play_launch 0cd95a0030aa
       but this `nros` was built from def315d4e479.
```

`def315d4e479` is a **nano-ros** commit, not a play_launch one. The resolver's
own `--version` correctly reports `play_launch 0cd95a0030aa…`.

Worse, the state is sticky. Initialising the submodule does not clear it, and
the remedy the message suggests (`just setup-launch-resolve`) cannot help
because the wrong value is on the **CLI** side. `just setup-cli` reports success
while rebuilding nothing.

## Cause — two independent faults that compound

**1. `git -C` walks up.** `packages/cli/nros-cli-core/build.rs` stamps the pin
with:

```rust
let play_launch = root.join("packages/cli/third-party/play_launch");
let pin = if play_launch.exists() {
    Command::new("git").args(["-C", …, "rev-parse", "HEAD"]) …
} else {
    "unknown".to_string()
};
```

An uninitialised submodule is an **empty directory that exists**, so the
`else` branch never runs — and `git -C <empty dir> rev-parse HEAD` walks up to
the enclosing repository and happily returns the **superproject's** HEAD. The
code's own intent (`"unknown"`) is unreachable in exactly the case it was
written for.

**2. It never re-stamps.** `build.rs` emits no `rerun-if-changed` for the
submodule, and `build.rs` itself is not in the source-stamp input list that
`setup-cli` freshness-checks. So after `git submodule update --init`:

* cargo sees no changed input → `build.rs` does not re-run;
* `setup-cli` sees an unchanged source stamp → reports success, rebuilds
  nothing;
* even `touch build.rs` is not enough.

Only touching a real `.rs` source file forces the correct pin to be recorded.

## Reproduction

1. Fresh clone (or `git submodule deinit packages/cli/third-party/play_launch`).
2. `just setup-cli` — succeeds, silently recording the superproject SHA.
3. `git submodule update --init packages/cli/third-party/play_launch`.
4. `just setup-launch-resolve` — succeeds, records the real play_launch SHA.
5. `nros sync` in any workspace → the guard fires and stays fired.

Hit during phase-338 W3 step 4 on a host where the submodule had never been
initialised.

## Fix

Both faults, together:

* **Require a repository, not a directory.** Gate on
  `play_launch.join(".git").exists()` (a submodule checkout has a `.git` *file*)
  rather than `play_launch.exists()`, or pass `--git-dir`/`rev-parse
  --show-toplevel` and verify it equals the submodule path. Either way an
  uninitialised submodule must yield `"unknown"`, which is what the code already
  intends.
* **Re-stamp when it appears.** Emit
  `cargo:rerun-if-changed=packages/cli/third-party/play_launch/.git` (and/or add
  it to the source-stamp inputs) so initialising the submodule invalidates the
  CLI build.

Worth considering: `"unknown"` on either side should probably be a distinct,
friendlier error ("play_launch is not initialised — run `git submodule update
--init …`") rather than being compared as if it were a SHA. The current message
sends the reader to the one command that cannot fix it.

## Note

The guard itself is good and did its job — it refused to run with a resolver it
could not vouch for, which is exactly what issue 0409 asked for. This is a
defect in how one side computes its value, not an argument against the check.

## RESOLVED (2026-08-05)

Both faults, in both stampers — `nros-cli-core/build.rs` and
`nros-launch-resolve/build.rs` carried the identical `.exists()` test, so fixing
only the reported side would have left the two disagreeing again.

- **Require a repository.** Gate on `<submodule>/.git` (a submodule checkout has
  a `.git` FILE) instead of the directory. An uninitialised submodule now yields
  `"unknown"` — the value the code always intended and the one the 0409 guard
  treats as unverifiable rather than as a mismatch.
- **Re-stamp when it appears.** Both emit
  `cargo:rerun-if-changed=<submodule>/.git`, so `git submodule update --init`
  invalidates the build instead of leaving the wrong pin stuck.

Verified end to end by moving the submodule's `.git` aside and back:

    .git absent   -> NROS_PLAY_LAUNCH_SHA=unknown        (was: the superproject sha)
    .git restored -> NROS_PLAY_LAUNCH_SHA=0cd95a0030aa…  (automatic, no touch of any .rs)

(The first restore attempt showed no re-stamp because `mv` PRESERVES mtime, so
cargo correctly saw nothing newer; a real `git submodule update --init` creates
the file fresh. Noted because the same false negative will mislead the next
person testing this by hand.)

Mine, from the issue-0409 direction-2 work.
