---
id: 288
title: "Self-contained standalone examples cannot be metadata-probed, so exact executor sizing never applies to them"
status: open
type: limitation
area: build, examples
related: [issue-0257, issue-0100, issue-0358]
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

## Option 4 — make the board dep optional; probe the LIB (2026-07-31)

Measured, and it makes options (1)–(3) mostly moot. The blocker is narrower
than "these packages cannot be host-compiled":

* the probe harness deps the component as `{ path = …, package = … }` — **default
  features** — and does `use <krate>::…`, so it needs a LIB target and gets
  whatever the default feature set drags in;
* the board dep is what cannot host-compile (ARM inline asm), and it is
  **unconditional** in every one of these manifests;
* but the NODE code does not use the board at all. In
  `examples/qemu-arm-baremetal/rust/action-client-rtic` — the package this issue
  cites as failing — `lib.rs` and `main.rs` mention the board **only in doc
  comments**. The dependency is pulled in by `nros::main!()` in the BIN, which
  resolves the board type.

So the split this issue's option (2) proposes largely already exists:

```
deploy-bound standalone rust examples : 88
  ... that already have a lib target  : 75   (85%)
  ... with the board dep optional     :  0
```

The change is per-manifest, not per-crate:

```toml
nros-board-rtic-mps2-an385 = { version = "*", optional = true }

[features]
default  = ["firmware"]
firmware = ["dep:nros-board-rtic-mps2-an385"]

[[bin]]
required-features = ["firmware"]
```

plus the harness depping the component with `default-features = false` and
building the lib.

**Verified on the failing example**: with the board dep made optional,
`cargo check --lib --no-default-features --target x86_64-unknown-linux-gnu`
completes cleanly. That is the package whose probe this issue records as dying
on ARM inline asm.

Remaining cost: 75 examples need ~4 manifest lines; 13 need a lib target
extracted first. That is materially cheaper than option (2)'s ~51 new crates,
and unlike option (3) it does not remove a real dependency behind cargo's back —
the bin still deps the board, declared, and `required-features` keeps a bare
`cargo build` honest.

Open question before doing it: whether `default = ["firmware"]` is right, or
whether the bin should be the only thing enabling it. Defaulting keeps
`cargo build`/`cargo run` working in a copied-out example, which is the whole
point of the standalone shape — but it also means the probe MUST remember
`--no-default-features`, which is a second thing to remember (cf. issue 0358).

## Cross-refs

* `docs/roadmap/phase-308-cpp-metadata-producer.md`
* `docs/issues/archived/0257-executor-max-cbs-not-derived-from-model.md`

## Update: the detection predicate was incomplete (2026-07-28)

The mechanism described above — detect deploy-bound packages, report them as
having no producer, degrade to the SystemModel bound — was correct but keyed on
`[package.metadata.nros.entry]` alone:

```rust
let deploy_bound = nros.entry.is_some();
```

`[deploy.<target>]` says the same thing, and **27** standalone examples spell it
that way instead (freertos, nuttx, threadx-linux, zephyr). They fell through to
the host probe and hit a hard build failure rather than this issue's graceful
degrade — issue 0318 is one instance, which presented as
`DOTCONFIG must be set by wrapper` on a Zephyr leaf.

Fixed in 0318 by accepting both spellings. That does **not** change this
issue's substance: those 27 packages still get the SystemModel's timer-blind
lower bound rather than an exact count. It only means they now degrade quietly,
as this issue always intended, instead of failing.

So the affected population is larger than "every `examples/*/rust/*` standalone
example that carries both tables" — measured today: **24** carry `[entry]` +
`[node]`, **27** carry `[deploy.*]` without `[entry]`. Both classes are
un-probeable and both now report as unsupported.

The choice between this issue's options (1) accept + document and (2) split the
shape is unchanged by that, but option (2)'s cost is ~51 examples, not ~40.

Worth noting the underlying smell for whoever takes this: **two spellings for
one fact**. `[entry]` and `[deploy.<target>]` both mean "bound to a deploy
target", and the predicate knowing only one is exactly the drift class this
repo has been closing elsewhere (two `system.toml` parsers, two entry emitters,
issue 0316's knob spellings, issue 0319's child-indexing rule). Collapsing them
to one declaration would prevent the next instance rather than fixing it.
