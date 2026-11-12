---
id: 2476
title: "A Zephyr pin move fails `west update` on every existing workspace: git refuses to check out over the patch set's in-place edits"
status: resolved
resolved: 2026-11-12
resolved_in: "sync-workspace-manifest.sh sets each west project's tracked edits aside to a diff before `west update`; the patch set re-applies ours after"
type: bug
severity: high
area: [zephyr, ci, setup]
related: [2249, 2389]
found: 2026-11-12
---

## Measured

The Zephyr 3.7 pin moved from v3.7.0 to v3.7.2 (#3619, merged 2026-11-12, for
issue 2389). From the next merge group on, the merge queue's L3 job (cross
build and link) failed on every group: 8 in a row after 12:48Z, the last
success at 12:28Z (run 43231918786, step `just setup tier2`):

```text
error: Your local changes to the following files would be overwritten by checkout:
        drivers/net/nsos_sockets.c ...
FATAL ERROR: command exited with status 1: checkout --detach bbbf8b30094d...
sync-workspace-manifest: west update failed (rc=1); restored the previous west.yml
```

The required `CI` aggregator stayed green, so PRs kept merging, but L3
produced no signal while red.

Reproduced locally on a patched v3.7.2 workspace, with a checkout whose
`west.yml` pins v3.7.0: main's `sync-workspace-manifest.sh` failed with the
same `would be overwritten by checkout` refusal and left `zephyr/` at v3.7.2.

## Cause

`scripts/zephyr/patches/<line>.sh` edits tracked files of west projects in
place: zephyr's `drivers/net/nsos_*.c`, `CMakeLists.txt` and a SoC file, and
zephyr-lang-rust's `CMakeLists.txt`, `Kconfig` and `dt-rust.yaml`.
`sync-workspace-manifest.sh` (issue 2249) runs `west update` when the
checkout's manifest differs from the workspace's copy. `west update` checks
the new revision out with git, which refuses to overwrite a modified file.

So the first pin move after a patched setup fails on every existing
workspace:

- **Users.** Anyone who runs `just zephyr setup` (or `scripts/zephyr/setup.sh`)
  on an existing workspace after pulling a pin move hits it.
- **CI.** The self-hosted runner's store workspace persists between runs, so
  its lane stays red until someone repairs the workspace by hand.

The 4.4 line goes through the same script and the same kind of in-place
patch set, so it had the same defect.

## Fix

Before `west update`, `sync-workspace-manifest.sh` handles every west project
that is its own git repository and has tracked edits:

- it writes the edits to `<ws>/.west/nros-preupdate/<project>.<time>.diff`;
- it resets the project to its HEAD;
- it names the project and the diff on the console.

Both callers run the patch set after the sync, and that re-applies our edits
to the new revision. A user's own edit is kept in the diff, never discarded.
If the update fails, the diffs are applied back, so a failed run leaves each
project as it found it.

A project directory that is not its own repository (the unbound manifest
project) is skipped. Without that check, `git -C` would walk up to an
enclosing checkout and reset that instead.

The fix covers every project and every patch at once. A list of the files the
patches touch would be a second copy of the patch set.

Verified:

- **Real Zephyr workspace.** Patched v3.7.2 → v3.7.0 → re-patched → v3.7.2 →
  re-patched, each sync rc 0. A sync to a nonexistent revision failed and
  restored the edits byte for byte, along with the manifest copy.
- **Gate.** `check zephyr-sync-workspace-manifest` gained a real-west row:
  an in-place edit, then a pin move, then a failed move. It passes 20/20 with
  the fix, and 18/20 with main's script, failing the two new pin-move
  assertions.

The self-hosted runner needs no hand repair. Its workspace copy was restored
to the old manifest after each failure, so its next sync takes the fixed path.
