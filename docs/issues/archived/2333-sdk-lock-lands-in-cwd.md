---
id: 2333
title: "`nros setup` writes `nros-sdk.lock` into the current directory; the book says `~/.nros` and never explains the lock"
status: resolved
type: bug
area: setup, book
purpose: [education, industrial]
related: [phase-523, issue-2258, issue-2321]
found: 2026-11-07
resolved_in: "adoption-r2-cli-ux branch: setup outside a project locks in the store; a workspace with no pin is its own project"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round two. Found 2026-11-07 on release `nros-0.6.0-nros7` in a pristine `ubuntu:24.04` with Jazzy, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n (runs: `docs/development/adoption-probes/runs/2026-11-07-round-2-{industrial,education}.md`).

## Measured

- E7: `nros setup mps2-an385-baremetal --rmw zenoh` run in `~` prints `locked in /home/dev/nros-sdk.lock`. The book promises `locked in /home/you/.nros/…`.
- As a result, two seats' workspace locks differ: one has an empty `[tool]`, because its tools were recorded in `~/nros-sdk.lock`. I4 saw the same.
- No page explains what `nros-sdk.lock` is or which one wins.

## Next

Outside a project, write the lock to the store, not the cwd, or refuse with a hint. Explain the lock and its project scope in Install.

## Resolution (2026-11-07)

"A project" is now one predicate, `store::project_of`: the nearest ancestor
holding a pin file (`nros-toolchain.toml`, `nros-sdk-index.toml`,
`nros-sdk.lock`), a workspace (`src/<pkg>/system.toml`) or a single-package
`system.toml`. A bringup at `<ws>/src/<pkg>/` belongs to `<ws>`.

- `nros setup` (`project_lock_path`) writes the project's lock inside a
  project, and the STORE's `<store>/nros-sdk.lock` outside one
  (`store::setup_lock_path_for`). The cwd fallback is gone from setup.
- `store::lock_path_for`, which `nros build`, `nros update` and the project
  lock use, takes the same predicate. A workspace before its first pin used
  to fall through to a pin file further up, a stray `~/nros-sdk.lock` or the
  nano-ros checkout it sat in, and record its tools there. Measured: a
  `nros new` workspace under a checkout recorded into the checkout's
  `nros-sdk.lock` before this, and into its own after.
- `nros store gc` consults the store's lock wherever it runs, like an
  installed toolchain's index (RFC-0103 D5), so what setup provisioned outside
  a project is not collected.
- The Install page shows `locked in /home/you/.nros/nros-sdk.lock` and says
  what the lock is, which one is written where, and that `nros update` moves a
  recorded version. RFC-0014's lock bullet is amended.

Issue 2258's rule stands: setup records into the lock and never moves a pin.
A `~/nros-sdk.lock` left by an older release is still a pin file and can be
deleted.
