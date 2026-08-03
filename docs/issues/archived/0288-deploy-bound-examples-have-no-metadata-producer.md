---
id: 288
title: "Self-contained standalone examples cannot be metadata-probed, so exact executor sizing never applies to them"
status: resolved
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

## Option 4 revised — gate the asm at the BOARD, not the example (2026-07-31)

The manifest approach above (board dep `optional` + a `firmware` feature on ~75
examples) was **not** taken. Measuring the actual failure showed the blocker is
one un-gated function, and the workaround would have sat as far as possible from
its cause while adding a Rust-specific feature convention to every example.

```
error: invalid register `r0`: unknown register
error: could not compile `nros-board-mps2-an385` (lib)
```

`semihosting_time()` names ARM registers in an `asm!` block with no `cfg`.
Gating it on `target_arch` (with an off-ARM stand-in that panics — the value
seeds entropy, so a silent `0` would collide GUIDs rather than fail) makes the
example host-compile **with no manifest change at all**.

Why it went unnoticed: these board crates sit in the workspace's `exclude` list
("embedded-only, require cross-compilation"), so no lane host-builds them. The
only consumer that tried was the probe, which reported the package as
unsupported instead of failing.

Verified both directions — `action-client-rtic` host (`x86_64`) and target
(`thumbv7m-none-eabi`), plus `threadx-qemu-riscv64`'s consumer on
`riscv64gc-unknown-none-elf`.

### What remains: the build-script layer

Gating the asm unblocks the **mps2/RTIC family only**. Seven board crates
cross-compile C/asm from `build.rs` and are still host-hostile one layer deeper:

| board crate | blocker |
| --- | --- |
| `nros-board-freertos` | `cc::Build` on FreeRTOS sources |
| `nros-board-mps2-an385-freertos` | same |
| `nros-board-nuttx-qemu-arm` | same |
| `nros-board-nuttx-qemu-riscv` | same (`nros-nuttx-ffi`) |
| `nros-board-orin-spe` | same; also an ungated `wfi`, left alone — its target build cannot be verified without the NVIDIA SDK |
| `nros-board-threadx` | `riscv64-unknown-elf-gcc` on ThreadX `.S`, invoked regardless of target |
| `nros-board-threadx-linux` | same |

`threadx` fails with `error occurred in cc-rs: … riscv64-unknown-elf-gcc …
tx_thread_interrupt_control.S` on a host build. The pattern to apply is the same
one the asm fix used — compile the target sources only when building for the
target — but it is a build-script change per crate, not a one-line `cfg`.

## The blocker is a STACK, not one wall (2026-07-31)

"These packages cannot be host-compiled" turned out to be four layers, each
hidden behind the one before it. Three are fixed; the fourth is a deliberate
stop.

| # | layer | symptom | status |
| --- | --- | --- | --- |
| 1 | ungated Rust `asm!` in board crates | `invalid register \`r0\`` | **fixed** — `cfg(target_arch)` |
| 2 | build scripts cross-compile C/asm unconditionally | `riscv64-unknown-elf-gcc … .S`, then `no such instruction: csrrci` | **fixed for every board an example deps** (prereq 1 done 2026-08-03; orin-spe parked, no example deps it) |
| 3 | `no_std` component + host default `panic = "unwind"` | `unwinding panics are not supported without std` | **fixed** — harness sets `panic = "abort"` |
| 4 | the probe SKIPS deploy-bound packages by declaration | reported `unsupported`, never attempted | **fixed** — skip lifted, best-effort + negative cache |
| 5 | board's platform C ABI (`nros_platform_*`) is host-skipped → probe fails at LINK | `rust-lld: undefined symbol: nros_platform_alloc` | **fixed** — harness deps `nros-platform-cffi[posix-c-port]` |

Layers 1–3 are verified on `examples/qemu-riscv64-threadx/rust/action-client`,
which now host-`cargo check --lib`s after failing at each layer in turn.

### Why layer 4 is not flipped

`metadata_refresh.rs` skips `deploy_bound` packages up front. Lifting that means
ATTEMPTING the probe, and the Rust path propagates failure (`build_metadata(…)?`)
whereas the C/C++ path records it as `unsupported`. So flipping the skip requires
making failure best-effort for these packages — easy — but also means six board
families that still cannot host-build would each spend a **full failing cargo
build per sync**, with nothing caching the negative result.

That trades a documented limitation for a build-time regression, which is the
wrong direction. The prerequisites, in order:

1. finish layer 2 for the remaining boards: `nros-board-freertos`,
   `nros-board-mps2-an385-freertos`, `nros-board-nuttx-qemu-arm`,
   `nros-board-nuttx-qemu-riscv`, `nros-board-orin-spe`,
   `nros-board-threadx-linux`;
2. give the probe a NEGATIVE cache keyed by source digest, so a package that
   cannot be probed is not re-attempted every sync;
3. then make deploy-bound failure best-effort and remove the skip.

Only after (1) and (2) does (3) pay for itself.

**Prerequisite 1 DONE (2026-08-03).** `freertos`/`mps2-an385-freertos`/
`nuttx-qemu-arm` were already host-gated (`host_probe::skip_cross_build`);
`nuttx-qemu-riscv` was the one left — its `build.rs` called `run_platform()`
with no guard and cross-compiled with `riscv*-elf-gcc` on a host with
`NUTTX_DIR` set. Added the same `skip_cross_build(…, &["riscv"])` guard, verified
it fires on `--target x86_64-unknown-linux-gnu`. `threadx-linux` needs no guard —
it is a host board (x86_64) and already `cargo build`s for the host.
`orin-spe` stays PARKED: its `fsp` feature deps `nvidia-ivc`, whose build.rs
needs the NVIDIA SDK (unverifiable here) and fails BEFORE the board's own gate
could run — and NO standalone example deps `orin-spe`, so it is not in the
affected population. So every board a deploy-bound example actually deps now
host-builds; prereq 1 is effectively complete.

**Note on prereq 2 with layer 2 done:** the negative cache's motivation was the
"full failing cargo build per sync" for un-host-buildable boards. With prereq 1
done, no deploy-bound EXAMPLE's board fails host-build anymore, so that cost is
~zero — prereq 2 is now defensive (a genuine component error, or a future
un-gated board) rather than load-bearing. Flipping layer 4 (prereq 3) is
therefore mostly unblocked; the remaining cost is the POSITIVE one — the first
sync after this would host-probe all ~48 deploy-bound examples (cached by
`sidecar_is_fresh` afterward). That perf/behaviour change is the open decision.

### Also fixed on the way

`nros-board-orin-spe` has the same ungated `wfi` and was deliberately left
alone: its target build cannot be verified here (`nvidia-ivc`'s build script
needs the NVIDIA SDK), and it is blocked by layer 2 regardless, so gating it
would be an unverified edit with no present benefit.

## Cross-refs

* `docs/roadmap/archived/phase-308-cpp-metadata-producer.md`
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

## RESOLVED — layer 4 flipped, layer 5 cleared (2026-08-03)

Prereqs 1+2 done, so prereq 3 (flip the skip) was taken. Flipping it uncovered
one more layer under the four — a fifth, at the link step, invisible until the
probe was allowed to reach it.

**Layer 4 — flip (`metadata_refresh.rs`).** Removed the up-front
`if decl.deploy_bound { …unsupported; continue }` skip, so a deploy-bound
package is now probed like any other. The Rust path's `build_metadata(…)?`
hard-fail is made best-effort **for deploy-bound packages only**: on failure it
records the package `unsupported` (the pre-flip degrade) instead of aborting the
sync; a regular package's failure still hard-fails. A **negative cache**
(prereq 2) keyed by source digest — an `<sidecar>.unprobeable` marker written
via `mark_unprobeable`, read by `is_known_unprobeable`, cleared by
`clear_unprobeable` on any successful/fresh probe — stops a genuinely
un-probeable package from re-spending a build every sync. Unit-tested
(`negative_cache_round_trip`: mark → known → digest-mismatch retry → clear).

**Layer 5 — the platform C ABI link wall.** With the skip lifted, the probe of
`examples/qemu-arm-baremetal/rust/action-client-rtic` compiled all 45 objects
(node, msgs, board, platform) and then died at LINK:

```
rust-lld: error: undefined symbol: nros_platform_alloc
rust-lld: error: undefined symbol: nros_platform_dealloc
```

`nros-platform-cffi`'s `CffiPlatform` **calls** ~90 `nros_platform_*` extern-C
symbols; the board crate's C build **defines** them — and that C is exactly what
`host_probe::skip_cross_build` skips on the host. So every board example would
link-fail and degrade, making the flip net-negative.

Fix: the probe harness (`render_harness_cargo_toml`) now deps
`nros-platform-cffi = { features = ["posix-c-port"] }`. `posix-c-port` compiles
the host-buildable `nros-platform-posix/src/platform.c`, which DEFINES the full
`nros_platform_*` ABI. Cargo feature-unifies it onto the same
`nros-platform-cffi` the component already pulls, so on the host it is the
**sole** definer (the board's C is skipped; the Rust `nros_platform_export!`
exporters live only in embedded platform crates whose `register()` is
cfg-gated off-target; `nros-platform-posix` has no `build.rs`, so it never
double-compiles `platform.c`). No duplicate-symbol risk.

**Verified end-to-end.** After `just setup-cli`, a fresh
`nros sync examples/qemu-arm-baremetal/rust/action-client-rtic` reports
`source metadata — 1 rebuilt` (was `no producer …`) and writes a real sidecar
with `"generator": "nros-metadata-rust"` and a provenance digest — exact
executor sizing, not the SystemModel timer-blind bound. Regular components
(native `talker`) still sync clean; the CLI metadata suite is green (109/109,
incl. the negative-cache test). Every deploy-bound example that host-builds now
gets exact sizing; one that cannot degrades best-effort and is negative-cached.

Fixed in `packages/cli/nros-cli-core/src/orchestration/{metadata_refresh.rs,metadata_build.rs}`.
