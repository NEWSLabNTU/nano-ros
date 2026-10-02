---
id: 1642
title: "The workspace toolchain is now pinned, and nothing tries the NEXT stable —
  so the pin can age silently until a routine six-week bump becomes a sixty-site
  one"
status: open
type: tech-debt
area: [ci, build]
severity: low
related: [1447, 1445]
found: 2026-10-02
---

## The gap

Issue 1447 was resolved by pinning `rust-toolchain.toml` to `channel = "1.99.0"`
(PR #1556), after Rust 1.99.0 reached every checkout and CI at once through the
floating `stable` channel and took `main` red for everyone.

The pin trades one risk for another. Before, a toolchain move arrived with no
commit and broke everyone the same day. Now a toolchain move happens only when
someone chooses to bump — and **nothing tells anyone that a bump is due, or how
large it will be.** Rust ships a stable release every six weeks. A pin left
alone for three releases arrives at its next bump carrying three releases of new
lints at once.

The measurement that prices this: clearing ONE release (1.98 → 1.99) took nine
sites across three tools — 2 clippy, 5 rustdoc, 7 rustc future-incompat errors
(PR #1536). That is the per-release cost, and it does not shrink by waiting.

## What would close it

An **advisory** lane that builds and lints the workspace against the next stable
toolchain — and is **allowed to be red**. Its job is to report, not to gate:

- red means "the next bump will cost these sites", visible before anyone pays it
- green means "the bump is free today", which is the moment to take it

It must NOT be merge-gating. A gating lane on an unpinned toolchain would
re-create exactly the failure 1447 just fixed — every PR blocked by a compiler
nobody chose.

Shape worth pricing: a scheduled job (not per-PR) running `check fast`'s clippy
and rustdoc constituents under `RUSTUP_TOOLCHAIN=stable`, posting its delta
against the pinned version. The two halves of the measurement already exist —
#1536's A/B (`RUSTUP_TOOLCHAIN=1.98.1` vs `=1.99.0` on the same tree) is the
template.

## What this is NOT

- **Not issue 1445.** 1445 asks which lanes gate merges. This asks for a lane
  that deliberately gates nothing. Conflating them would put an unpinned
  toolchain in the merge path.
- **Not a reason to un-pin.** The pin stands; this is what makes keeping it cheap.
