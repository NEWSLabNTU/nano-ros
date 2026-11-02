---
id: 2216
title: "The entry-root clean (issue 2167) stops `nros build` on every fresh fixture tree: `cargo clean` refuses a target dir with no CACHEDIR.TAG"
status: resolved
type: bug
severity: high
area: [cli, build]
related: [2167]
found: 2026-11-02
---

## What

Issue 2167's fix runs `cargo clean -p <entry>` when an entry's cargo root
changes. Building threadx-linux fixtures in a fresh worktree
(`just threadx_linux build-fixtures`, 2026-11-02) failed on two workspace rows:

```
error: cannot clean `…/examples/workspaces/rust/target-fixtures/threadx-linux`: missing or invalid `CACHEDIR.TAG` file
Error: cleaning the entry failed (exit status: 101): cargo clean -p threadx_entry …
  !! workspace-rust-threadx-linux: nros build failed (rc=1)
```

Two defects, both in 2167's fix:

1. `EntryRoot::changed` read "no marker, but the target dir has content" as
   "built before, root unknown, clean once". That content is not evidence of
   a cargo build. nros writes `nros-class/<hash>/sizing.toml` into the target
   dir before cargo runs, so every fresh tree counted as a root change.
2. `cargo clean` refuses a target dir that has no `CACHEDIR.TAG`. With a
   separate `build.build-dir`, which the fixture road always passes, cargo
   writes that tag into the build dir only. Measured with cargo 1.99 on a
   scratch crate: a real build, then `cargo clean -p` on the same target dir,
   exits 101. So the clean could never succeed on that road, even for a real
   root change.

2167's end-to-end check passed because it ran on a `nros build` whose target
dir nros had not pre-populated.

## Resolution

- `changed` is true only when a marker exists and names a different root. A
  target dir with no marker is recorded, never cleaned on a guess. This gives
  up one case: a tree built before the marker existed, from the other root.
  The cost there is one stale build, which the next root change catches.
- Before cleaning, `EntryRoot::tag_target_dir` writes the standard cache
  directory tag into the target dir nros chose, unless one is already there.
  The directory is a build cache nros owns, which is what the tag declares.
  Measured: the same `cargo clean -p` exits 0 once the tag is present.
- Tests: `an_entry_root_change_is_seen_until_it_is_recorded` (nros's own
  files no longer count as a change) and
  `the_target_dir_is_tagged_for_cargo_clean_once`.
