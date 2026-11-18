---
id: 2695
title: "nros build reports every store source as drifted when the store sits inside a git repository"
status: resolved
type: bug
severity: medium
area: [cli, build, provisioning]
related: [phase-578]
found: 2026-11-18
resolved_in: "fix(cli): a store source inside a git repository is not that repository's checkout (issue 2695)"
---

## What happened

Measuring phase-578 W8's first build, with `NROS_HOME` set to a fresh store
inside a nano-ros checkout, every `nros build` finished compiling and was then
refused:

```text
Error: the build finished, but it resolved off the pin set.
3 dependency drift(s) from the pin set:
  source `freertos-kernel` is at fe864d75…, pinned at 0adc196d…
  source `lwip` is at fe864d75…, pinned at 96b63e31…
  source `zenoh-pico` is at fe864d75…, pinned at c05434a7…
```

`fe864d75` was the enclosing checkout's HEAD. A store source is an extracted
tree with no `.git` of its own, and `project_lock::git_commit` ran
`git -C <dir> rev-parse HEAD`, which walks UP to the nearest repository. So
any store under any git repository reads every source as drifted to that
repository's commit, and refuses every build. A home directory kept in git
(a common way to manage dotfiles) puts the default store, `~/.nros`, exactly
there.

## Fix

`git_commit` returns `None` unless `dir` has its own `.git` (`exists()`, so a
submodule's `.git` file counts, issue 1336). That is the guard
`source_stamp::play_launch_pin`, `nros-build-paths`' pin check and the
launch-resolve build script already use. The test builds a repository holding
a store-shaped tree and asserts the tree has no commit while the repository
has one; without the guard it fails.

Sweep: `git grep -nE '"rev-parse",\s*"HEAD"|\["rev-parse", "HEAD"\]' -- 'packages/**/*.rs'`.
The other production sites either check `.git` first or read a fresh clone
(`sdk_store`'s source-build provenance).
