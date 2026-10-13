---
id: 1859
title: "`derived_tiers_bake` fails 6/6 in the compile tier: its `copy_tree` meets a non-regular entry in `examples/workspaces/derived-tiers-cpp`"
status: resolved
resolved: 2026-10-13
type: bug
area: [ci, testing, cli]
severity: medium
found: 2026-10-12
related: [1353]
---

## Measured

Gate run [38312520026](https://github.com/NEWSLabNTU/nano-ros/actions/runs/38312520026)
(`workflow_dispatch` on main at `710cc5a79`), job 115021462187,
"check / build tier + no-std (nightly/manual only)":

```
check-build (parallel): 1 of 25 gate(s) FAILED
===== FAIL (cli-tests, rc=101, 681545ms) =====
thread 'the_bake_reports_the_derived_tiers_and_binds_every_node' panicked at nros-cli-core/tests/common/derived_tiers.rs:164:42:
copy: Error { kind: InvalidInput, message: "the source path is neither a regular file nor a symlink to a regular file" }
test result: FAILED. 0 passed; 6 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s
error: test failed, to rerun pass `-p nros-cli-core --test derived_tiers_bake`
```

All six `derived_tiers_bake` tests fail at the same line: the `fs::copy` arm of
`copy_tree` (`packages/cli/nros-cli-core/tests/common/derived_tiers.rs:156`).
The tree being copied is `examples/workspaces/derived-tiers-cpp`
(`derived_tiers.rs:74`).

## What it is, and what it is not

`copy_tree` recurses on `file_type().is_dir()` and calls `fs::copy` on
everything else. `DirEntry::file_type` does not follow symlinks, so a symlink to
a DIRECTORY, a socket, a FIFO or a dangling link all fall through to `fs::copy`,
which refuses them with exactly this message. The tracked tree has no such entry
(`git ls-tree` shows no mode `120000`, and a clean checkout holds only
`README.md` and `src/`). So something earlier in the same job wrote one. The one
earlier step in the log that touched this workspace is the build tier's
`sync: examples/workspaces/derived-tiers-cpp`. The lead, not measured: a path
that `nros sync` (or something it runs) leaves under `build/`.

It does not fail on the pull-request lane. `check / cli-tests` runs there on a
clean checkout, so the copied tree is clean. The build tier runs only on
`schedule` and `workflow_dispatch`, after its own `sync` pass.

It is NOT 1353, though 1353 hid it. The three scheduled gate runs before this one
(38195329902, 38104415110, 38016065885) failed in the same job. 38195329902's
annotation is the runner dying with `No space left on device`, so it never
reached `cli-tests`. How long this has been red is therefore unknown.

## What would close it

1. Run `nros sync` on `examples/workspaces/derived-tiers-cpp` and list
   `find … ! -type f ! -type d` to name the entry.
2. Then fix the class rather than the one entry. Either the test copies only
   what a fresh checkout has (skip `build/`, or copy `git ls-files`), or sync
   stops leaving a directory symlink in a source tree. The first is the
   test-side fix: a copied fixture should never depend on what an earlier gate
   built beside it.
3. Acceptance: `just check build` (or a manual gate dispatch) with
   `derived_tiers_bake` 6/6 after a `sync` of that workspace.

## Resolution (2026-10-13)

Fixed by #2087: the test now copies only the fixture's sources, not its build output. Whatever `sync` leaves under `build/` can no longer reach `fs::copy`. Acceptance (a build-tier `derived_tiers_bake` 6/6 after a `sync`) still comes from the next scheduled gate run.
