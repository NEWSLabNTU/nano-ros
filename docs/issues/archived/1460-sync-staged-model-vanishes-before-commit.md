---
id: 1460
title: "`nros sync` fails on its own staging file — `<model>.yaml.resolving` is
  gone by the time the pin is stamped or the rename runs, at a different package
  each attempt, and the same sync succeeds when re-run"
status: resolved
type: bug
area: [cli, tooling]
severity: high
found: 2026-09-22
related: [0409, 0427, 1420, phase-439]
---

**Home: [phase-439](../../roadmap/phase-439-resolve-before-configure.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

## What happens

On a fresh worktree of `main` (`9722fca32`), `just rust-rtos-link-check` dies in
its `_codegen` dependency, in `scripts/regenerate-bindings.sh`'s serial
`nros sync` loop. Twice in a row, at a DIFFERENT package each time, and at a
different point in the same short code path:

**Attempt 1** — `examples/templates/local-msg-package`, stamping the pin:

```
Error: sync: stamp resolver pin for `rust_consumer`
Caused by:
   0: read staged model …/build/nros/models/rust_consumer/system_model.yaml.resolving
   1: No such file or directory (os error 2)
Location: nros-cli-core/src/cmd/ws.rs:1497:10
```

**Attempt 2** — `examples/workspaces/c`, committing the rename:

```
Error: sync: commit resolved model …/build/nros/models/demo_bringup/multihost_robot1_model.yaml
Caused by:
    No such file or directory (os error 2)
Location: nros-cli-core/src/cmd/ws.rs:2348:18
```

Both are the SAME file — `model.with_extension("yaml.resolving")` — missing
where the code has just been told it exists. The sequence at `ws.rs:2320-2348`
is: run `nros-launch-resolve -o <staged>`, bail if it exited non-zero (that path
removes `<staged>` itself), `verify_params_projected(<staged>)`, then
`stamp_resolver_pin(<staged>)` (reads and rewrites it), then
`std::fs::rename(<staged>, <model>)`. Attempt 1 failed at the read, attempt 2
got past both the verify and the stamp and failed at the rename.

**Re-running the same sync succeeds.** `nros sync examples/templates/local-msg-package`
immediately afterwards completed and left `system_model.yaml` in place;
`examples/workspaces/c` likewise, and all seven of its models —
`multihost_robot1_model.yaml` included — are present now. So each attempt got
further than the last, which is what makes this expensive rather than merely
noisy: the lane has to be run once per package that trips it.

## What this is NOT

- **Not the resolver failing.** Its exit status is checked immediately above
  and a non-zero exit takes a different branch with a different message; both
  failures are about the staged FILE after a successful resolve.
- **Not issue 0409's "missing data".** That check (`verify_params_projected`)
  has its own wrapper text and attempt 2 passed it before dying at the rename.
- **Not the ABI guard.** `nros sync ABI version guard bypassed via
  NROS_SKIP_VERSION_CHECK=1` is a warning the recipe sets deliberately.
- **Not obviously my own concurrency, though attempt 1 cannot rule it out.**
  Hand-run `nros sync` calls on other leaves overlapped attempt 1. Attempt 2 had
  none, and `regenerate-bindings.sh` syncs serially — so if something is racing,
  it is inside one `nros sync` process, not between them.

## Why it matters

`_codegen` is a dependency of `rust-rtos-link-check` and of the `ci` ladder, so
a lane can fail here having measured nothing, with an error that names a
temporary file rather than anything a reader can act on. It is invisible in CI
for the opposite reason to most flakes: the merge queue's L3 job gets PAST
codegen every time, so nothing in CI has ever reported this.

## What would close it

`nros sync` completing on every package in `regenerate-bindings.sh`'s loop on a
fresh checkout, first attempt, repeatedly — and, whatever the mechanism turns
out to be, the staged path no longer being a name two things can reach. The
first measurement worth taking is whether one `nros sync` process resolves
several models concurrently: two variants of one bringup
(`multihost_robot1`/`multihost_robot2`) share a directory, and attempt 2 died on
the first of that pair.

## Resolution (2026-10-23)

**Cause: two `nros sync` processes resolving the same model.** The staged path
was `model.with_extension("yaml.resolving")`, one name per model, so every
concurrent resolve of that model wrote, read and renamed the SAME file. The
first to finish renamed it away; the others then failed reading it (the pin
stamp) or renaming it (the commit). There is no in-process race: one sync
resolves its models serially, `nros-launch-resolve` writes `-o` with a plain
`fs::write`, and nothing else in the sync path deletes the file. The "re-run
succeeds" behaviour is the race not recurring, not progress.

**Reproduced.** In `examples/workspaces/c` (seven models), each round touched
`src/demo_bringup/system.toml` so every model re-resolved, then started four
`nros sync` processes at once. Ten rounds with the old name: **22 of 40 syncs
failed**, with both errors this issue records (`stamp resolver pin` /
`read staged model … No such file or directory`, and `commit resolved model`)
and a third it did not: `parse staged model …: YAML: missing field meta` — a
reader reaching the staged file while another process was mid-write. That
one is worse than a failure, because the same read can also succeed on a
file another resolve is about to overwrite.

**Fix.** `staged_model_path` names the staged file
`<model>.resolving.<pid>.<seq>`, so no two resolves, in one process or
several, can reach one staged file. The promotion is still a same-directory
`rename`, so it stays atomic, and racing resolves of one model now end with
one complete model in place. Every reader of the models directory selects by
suffix (`_model.yaml`, `.census.json`), so a name left by a killed process is
never read as a model. Same ten rounds with the fix: **0 of 40 failed**,
every log showing real `sync: resolved` lines, and no staged file left
behind. Unit test: `staged_model_path_tests`.

Where the concurrency came from in the original two attempts is not
established — attempt 1 overlapped hand-run syncs; attempt 2's second
process is unknown. The fix does not depend on knowing it.
