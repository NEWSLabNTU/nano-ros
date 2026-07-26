---
id: 288
title: "Self-contained standalone examples cannot be metadata-probed, so exact executor sizing never applies to them"
status: open
type: limitation
area: build, examples
related: [issue-0257, issue-0100]
---

## Finding (phase-308, 2026-07-26)

The source-metadata probe compiles the component for the HOST — that is what
makes one probe cover every deploy target. A package that declares its node
AND its entry in one crate (the issue-0100 "self-contained standalone example"
shape) deps its board crate directly, so it cannot be host-compiled at all.
Probing `examples/qemu-arm-baremetal/rust/action-client-rtic` failed compiling
ARM inline asm in `nros-board-mps2-an385`.

These packages are now detected (`[package.metadata.nros.entry]` present
alongside a node ⇒ deploy-bound) and REPORTED as having no producer, rather
than failing the build. Their executor sizing falls back to the SystemModel's
timer-blind lower bound — which is exactly the pre-phase-307 behaviour, so
nothing regresses; they simply do not get the exact count.

Affects every `examples/*/rust/*` standalone example that carries both tables.

## Why it matters

Issue 0257's failure mode — an executor sized too small dies at boot with
`code=-6 Full` — is still reachable for these packages, because the model
cannot see their timers. They are small demos today, so the four-slot default
covers them; the risk is a user copying one as a template and growing it.

## Options

1. **Accept and document.** The canonical shape for anything non-trivial is
   the workspace Node pkg, which IS probeable. Say so in the examples README
   and in the book's standalone-example page.
2. **Split the shape.** Give each standalone example a lib-only node crate the
   entry deps, which is the workspace shape minus the workspace. Costs one
   extra crate per example (~40 of them) and changes a documented layout.
3. **Probe with the board dep cfg'd out.** Fragile: the board dep is a real
   dependency, not a feature, and removing it changes what `register()` sees.

(1) is the cheap correct answer if the limitation is documented where a user
copying an example will read it. (2) is the thorough one and would also make
the examples demonstrate the shape we actually recommend.

## Cross-refs

* `docs/roadmap/phase-308-cpp-metadata-producer.md`
* `docs/issues/archived/0257-executor-max-cbs-not-derived-from-model.md`
