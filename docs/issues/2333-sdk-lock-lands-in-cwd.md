---
id: 2333
title: "`nros setup` writes `nros-sdk.lock` into the current directory; the book says `~/.nros` and never explains the lock"
status: open
type: bug
area: setup, book
purpose: [education, industrial]
related: [phase-523, issue-2258, issue-2321]
found: 2026-11-07
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round two. Found 2026-11-07 on release `nros-0.6.0-nros7` in a pristine `ubuntu:24.04` with Jazzy, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n (runs: `docs/development/adoption-probes/runs/2026-11-07-round-2-{industrial,education}.md`).

## Measured

- E7: `nros setup mps2-an385-baremetal --rmw zenoh` run in `~` prints `locked in /home/dev/nros-sdk.lock`. The book promises `locked in /home/you/.nros/…`.
- As a result, two seats' workspace locks differ: one has an empty `[tool]`, because its tools were recorded in `~/nros-sdk.lock`. I4 saw the same.
- No page explains what `nros-sdk.lock` is or which one wins.

## Next

Outside a project, write the lock to the store, not the cwd, or refuse with a hint. Explain the lock and its project scope in Install.
