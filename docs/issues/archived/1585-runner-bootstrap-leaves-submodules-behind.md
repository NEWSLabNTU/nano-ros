---
id: 1585
title: "`runner-bootstrap` moves the store checkout to a new commit and leaves its submodules behind, so every `just setup` step refuses the pin"
status: resolved
resolved: 2026-09-29
type: bug
area: [ci]
severity: medium
found: 2026-09-29
related: [1166, 1353]
---

## What happened

Re-bootstrapping the contained runner after moving its store to an SSD
(2026-09-29) exited 1. Every `just setup` step that `runner-provision.sh` ran
refused with:

```
packages/cli/third-party/play_launch is at 0647131a, not the pin this tree records
```

The runner came back online anyway, because its tools had been provisioned on
an earlier run and `runner-doctor` does not depend on the submodule. The next
re-provision of any store checkout older than the last `play_launch` bump fails
the same way.

## Cause

`scripts/ci/runner-bootstrap.sh` refreshes an existing store checkout with

```sh
git -C "$SRC" fetch --depth 1 origin "$REF"
git -C "$SRC" checkout -q FETCH_HEAD
```

and never touches submodules. A submodule that an earlier bootstrap
initialised stays at its OLD commit, while the superproject now records a new
pin. The fresh-clone branch has the mirror problem: it never initialises
`play_launch`, which provisioning needs.

## Reproduced

On a scratch shallow clone:
1. Check out `011d4a793a` (the store's commit) and init `play_launch`. It is at
   `0647131`.
2. Run the script's own `fetch --depth 1 origin main` + `checkout FETCH_HEAD`.
3. `git submodule status` reports `+0647131… packages/cli/third-party/play_launch`:
   behind the pin, the exact state `just setup` refuses.

## Resolution

After the clone-or-fetch, the bootstrap now runs:
- `git submodule sync`;
- `git submodule update --depth 1`, which moves every INITIALISED submodule to
  its pin;
- `git submodule update --init --depth 1 packages/cli/third-party/play_launch`,
  non-recursively, per CLAUDE.md, since its runtime submodules are never built here.

On the same scratch checkout this moved `play_launch` to main's pin `bbf9c04`,
left zero submodules behind (`git submodule status | grep -c '^+'` → 0), and
left `play_launch`'s own nested submodules uninitialised.

**Not verified:** a full `runner-bootstrap` run inside the container with this
change. The runner is busy on a job, and a bootstrap shares its volumes. The
change is shell only. The script and the extracted inner `BOOTSTRAP_SH` block
both pass `bash -n`, and `--check` prints the updated plan.
