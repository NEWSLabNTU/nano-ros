---
id: 2331
title: "The education route's pages cite RFC numbers and link into the repository instead of staying in the book"
status: resolved
type: enhancement
area: book
purpose: [education]
related: [phase-523, phase-537]
found: 2026-11-07
resolved: 2026-11-07
resolved_in: "book-r2-followup branch: the education route links nothing in the repository"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round two. Found 2026-11-07 on release `nros-0.6.0-nros7` in a pristine `ubuntu:24.04` with Jazzy, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n (runs: `docs/development/adoption-probes/runs/2026-11-07-round-2-{industrial,education}.md`).

## Measured

- E2: Choose your path › Design background and First Project › Scaffold and Design background (also Project layout § `src/lib.rs`) say "The three purposes and their budgets are RFC-0116 D1". They link `github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/…` and `examples/…` in the repository.
- `user-surface-ids` forbids the `RFC-NNNN` spelling in body text, so these sit in a "Design background" form the gate allows.

## Next

The book work (phase-537 owns the user book): keep the user book self-contained, and move the design background to the Internals book behind one link.

## Resolution

Principle 5 of the Book charter decides the RFC half: an RFC number may stay,
as plain text, in a page's closing "Design background" and nowhere else. The
repository half is dropped.

- **Repository links.** Every `github.com/…/docs/design/…` link in the
  Tutorial's ten chapters (`book/src/tutorial/`) is now its plain RFC number,
  in the same "Design background" sentence. Choose your path and Welcome
  already named theirs as plain text.
- **The example link.** Tutorial 1's pointer to
  `examples/mps2-an385-baremetal/rust/talker/` in the repository is gone. The
  example it pointed at is the talker the reader just scaffolded, so the page
  says to run the same `nros new` line with another workspace name to keep an
  untouched copy. Its "Design background" sentence about that pointer went
  with it.
- **RFC numbers in body text.** None remain before a page's "Design
  background" in `book/src/start/`, `book/src/tutorial/` or
  `book/src/starters/`.

Moving the design background to the Internals book behind one link, the
issue's other idea, was not done: Principle 5 keeps it on the page, as the
reader's one way to find the decision a page follows.
