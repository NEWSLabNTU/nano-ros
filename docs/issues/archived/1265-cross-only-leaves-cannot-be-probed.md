---
id: 1265
title: "The metadata probe cannot run for a cross-only leaf, so esp32 and mps2 examples must DECLARE their entities by hand"
status: resolved
resolved_in: "PR #1527 (2026-10-01)"
type: tech-debt
area: [tooling, build]
related: [1061, 1142, 1555, 1556, 1601, 1602, 1603, 0827, 0939, rfc-0098, phase-445]
---

## What

`nros sync` learns what a Rust component creates by building a METADATA PROBE
of it for the HOST and reading `<leaf>/metadata/<component>.json`. Pool sizes
(`ZPICO_MAX_SUBSCRIBERS`, the executor arena, …) are derived from that file
(issue 0827).

A leaf the host cannot build gets `<component>.json.unprobeable` instead. Two
shapes cause it (`leaf_entity_env.rs`):

- a foreign `[build] target` with `[unstable] build-std` — the esp32-c3 leaves
  (`riscv32imc-unknown-none-elf`, `build-std = ["core", "alloc"]`);
- a board crate with no host build — the mps2-an385 bare-metal leaves.

For those, issue 1061's fix has the leaf DECLARE its entities — today on its
`system.toml` `[[component]]` row (`entities = [...]`, RFC-0098 D8), same
grammar as `nano_ros_node_register(... ENTITIES ...)`.

(This paragraph used to give the home as `[package.metadata.nros.component]
entities`. That moved in phase-445 W3 and phase-454 W9 made the manifest key a
hard REFUSAL, so the spelling this issue named no longer parses. Corrected
2026-09-29.)

## Scope: the three ENTITIES populations (measured 2026-09-29)

`ENTITIES` is not one thing, and this issue owns one third of it. Retiring "the
declaration" means retiring three surfaces with three different blockers:

| # | Surface | State | Blocker | Owner |
| --- | --- | --- | --- | --- |
| 1 | the cmake `ENTITIES` keyword on `nano_ros_node_register()` | **RETIRED** (phase-412). Still parsed only to raise a migration `FATAL_ERROR` naming the contract sidecar; the metadata splice point went in phase-454 W9 | none — the guard is the last of it | phase-412 / phase-454 W9 |
| 2 | the `"entities"` key of `nros-metadata.json` | **no production producer.** 255 built metadata files scanned, 0 carry the key, so the reader yields `Declaration::Absent` on every real road | the reader survives because the declared-QoS C/C++ compile fixture is a committed metadata document, and the only other input (`--model`) cannot be committed — `check-no-tracked-models` | **issue 1555** |
| 3 | `system.toml` `[[component]] entities` | **LIVE**, 14 leaves | splits 2 / 12 by reason — see below | **this issue** (2) + **issue 1556** (12) |

Population 3 splits, and the split is the reason this issue does not cover all
of it:

* **2 leaves — `examples/esp32-c3-baremetal/rust/{talker,listener}`.** The probe
  EXISTS for them and cannot run. That is this issue.
* **12 leaves — `examples/qemu-armv7a-nuttx/{c,cpp}/*`.** Nothing REACHES them:
  the C/C++ probe is workspace-scoped and these are standalone cmake leaves with
  no `Cargo.toml`, no `metadata/` dir and no `.unprobeable` marker, and having
  no bringup they have no SystemModel either. Issue 1142 (resolved, phase-412
  W5) gave them the declaration and measured it worth −31,136 B of
  `SERVICE_BUFFERS` on one leaf. This issue's fix direction — read the entities
  from the cross-compiled artifact — does not help them, because the problem is
  not that the host build fails. **Issue 1556.**

Also corrected here: the title and the shape list below say "mps2" leaves must
declare. **No mps2 leaf declares `entities` today** — the declaring set is
exactly the 2 esp32-c3 and the 12 NuttX leaves above. Whether the mps2
bare-metal leaves are correspondingly under-sized, or are served by the kept
`NROS_DECLARED_*` carriers, is unmeasured; the same open question applies to the
six `examples/zephyr/rust/*` leaves, which are `.unprobeable` and declare
nothing. Recorded in 1556's "A gap this measurement turned up".

## Why it is a workaround, not a fix

The declaration is cross-checked against the probe **only where the probe
runs**. On exactly the boards that need it, nothing checks it, so it can go
stale the moment someone adds a subscription and forgets the list. A stale
declaration under-sizes a pool, and on esp32-c3 a pool sized wrong is not a
graceful error: `.stack` is the linker leftover after `.bss` (issues 0190,
1052), and `ZPICO_MAX_*` sizes fixed C arrays where too small is a
registration failure at boot.

It also puts a node's contract in `Cargo.toml`, which RFC-0098 reserves for
Rust-toolchain facts. Phase-445 W3 moves the declaration to `system.toml`
`[[component]]` (RFC-0098 D8) — a better home for the same workaround, not the
end of it.

## Fix direction (not decided)

Read the entities from the artifact that IS built — the cross-compiled leaf —
rather than from a host rebuild of it. Candidates, none measured yet:

- the component emits its entity table into a dedicated, `#[used]` link
  section (the `__NROS_SIZE_*` / `__NROS_LU_SZ_*` markers already use this
  shape — `nros-sizes-build::extract_sizes` reads symbol SIZES from an rlib
  without running anything), and sync reads it from the target `.rlib`;
- a host shim of the board crate, so the probe links on the host.

Acceptance: an esp32 and an mps2 leaf with NO declared entities get the same
derived pool sizes as today, and adding a subscription to either changes them
without touching a declaration.

## A third shape, and it is UNCLASSIFIED rather than refused (2026-09-23)

The two shapes above are caught by `probe_blocker`, which names the reason and
degrades cleanly. A scaffolded project reaches neither. Seen in every scheduled
`gate` run — job `nros new -> sync -> resolve`, which PASSES, e.g. run
35808783184 job 107015560271:

```
sync: source metadata — no producer for uj_demo::uj_demo (deploy-bound probe
  failed: metadata-mode harness failed (exit 101) for component 'uj_demo':
  error: invalid instruction mnemonic 'bkpt')
sync: 1 component(s) are un-probeable, so pool budgets stay at the crate
  defaults (issue 1061): uj_demo.json.unprobeable
```

`nros new` scaffolds a **baremetal** project. `probe_blocker` does not recognise
it, so instead of a refusal that says "this leaf cannot be host-built" the
harness is compiled for the host and dies on ARM inline assembly — `bkpt` fed to
an x86 assembler. The outcome is the same degradation (`.unprobeable`, crate
defaults) and the same silent under-sizing risk this issue is about, reached by
falling over rather than by a decision.

Two things that makes worse than the classified shapes:

* the message names an assembler mnemonic, so it reads as a toolchain bug. It
  cost one investigation in phase-466 W4, where it was the only legible error in
  a scheduled `gate` run whose actual failure (issue 1353, disk) had destroyed
  its own log;
* it is on the **scaffolding** path, which is the first thing a new user runs.
  `probe_blocker` returning a reason here would make the front door say what it
  is doing; today it says `bkpt`.

Not the fix this issue is waiting for — the fix is still "read the entities from
the artifact that IS built" — but classifying this shape is independently worth
doing, and cheap.

## Resolution (2026-10-01)

**Neither candidate in the fix direction -- a third one, cheaper than both: the
probe could not build these leaves because their PACKAGE named crates only the
IMAGE needs.** The probe compiles the leaf's LIB for the host, and cargo resolves
a package's whole `[dependencies]` for its lib, so `esp-hal`, `esp-backtrace`,
the board crate, `cortex-m(-rt)` and `panic-semihosting` -- none of which
`lib.rs` names -- were compiled for x86 too. The measured failures were exactly
those crates: `portable_atomic_unsafe_assume_single_core ... not supported on
this architecture` (esp32-c3, via esp-hal) and `invalid instruction mnemonic
'bkpt'` (mps2 and the `nros new` scaffold, via cortex-m semihosting). `main.rs`
(the Entry) is never built for the host, so those crates move to
`[target.'cfg(target_os = "none")'.dependencies]` and the node lib builds
everywhere. Nothing is read from the cross artifact and no board shim exists.

What changed:

* **`examples/esp32-c3-baremetal/rust/{talker,listener}`** -- image-only deps
  target-scoped; `lib.rs` logs through `nros::log_info!` / `nros::get_logger`
  instead of the board's `nros_log` re-export (same crate, now reached without
  the board); the `entities` lists are DELETED from `system.toml`.
* **`examples/mps2-an385-baremetal/rust/{talker,listener,serial-talker,
  serial-listener,talker-xrce}`** -- the same target-scoping. These declared
  nothing and were at the crate defaults; they now derive.
* **The `nros new` baremetal/esp32 template** (`cargo-nano-ros/src/scaffold.rs`)
  -- the same target-scoping, so the "third shape" above is not classified but
  removed: a scaffolded project PROBES. Cross-building the scaffold for the first
  time also found two template bugs that had nothing to do with probing and had
  kept every baremetal/esp32 scaffold from compiling: `nros::main!()` emitting a
  second `panic_impl` beside the crate's own panic handler (E0152, now
  `panic = "own"`, as every example spells it), and an esp32 `main.rs` calling
  `esp_hal::esp_app_desc!()`, which does not exist (E0433, now the board's
  `esp_bootloader_esp_idf` re-export). The lib also asks for `nros/macros`
  itself instead of inheriting it from the board.
* **Two manifest readers that skipped `[target.*]` tables**, found because the
  first esp32 sync after the move silently dropped the board's
  `[patch.crates-io]` row (the cross build would have resolved
  `nros-board-esp32-qemu = "*"` against crates.io): `ws::registry_style_dep_names`
  (feeds the leaf patch rows via `build::registry_patches_from`) and
  `nros-build`'s `board_framework::resolve_board_crate` (the out-of-tree
  `emit_board_framework` seam). Its sibling `extract_consumer_registry_nros_deps`
  already walked them -- two readers of one manifest disagreed. One test each.
* **`scripts/ci/scaffold-journey-check.sh`** (the scheduled `gate` job
  `nros new -> sync -> resolve`) now FAILS unless the scaffolded node was probed
  (`metadata/<name>.json` present, no `.unprobeable`). Before, the job passed
  while printing the `bkpt` line quoted above.

### Measured

| leaf | before | after |
| --- | --- | --- |
| esp32 talker | `.unprobeable`; pools from `entities = [publisher, timer]` | probed; `build/esp32-c3-baremetal/nros-cargo.toml` **byte-identical**; sizing descriptor byte-identical |
| esp32 listener | `.unprobeable`; pools from `entities = [sub]` | probed; `nros-cargo.toml` **byte-identical**. The descriptor gains one OBSERVED fact: the subscription's `registration_path = "in_place"` (was refused, issue 1522) |
| mps2 talker / listener | `.unprobeable`, nothing declared: every pool at the crate default, descriptor `status = "refused"` | probed; e.g. talker `NROS_EXECUTOR_MAX_CBS=1`, `ZPICO_MAX_PUBLISHERS=1`, listener `NROS_RMW_SUBSCRIBER_SLOTS=1`; descriptor `status = "partial"` with counts stated |
| mps2 serial-talker / serial-listener / talker-xrce | same as above | probed |

Declared-vs-probed agreement was also checked the way the tree checks it: the
first sync after the move ran with the esp32 declarations STILL PRESENT, and
`leaf_entity_env::reconcile` (which refuses on any per-kind disagreement)
accepted both.

Acceptance, both halves:

* *no declaration, same pools*: the esp32 rows above.
* *adding a subscription changes them without touching a declaration*: adding
  `create_subscription_for_callback_name::<StringMsg>("on_echo", "/echo")` to the
  esp32 talker's `register` and re-syncing moved `NROS_EXECUTOR_MAX_CBS` 1 -> 2,
  `NROS_RMW_SUBSCRIBER_SLOTS` 0 -> 1 and the descriptor's `subscriber_count` /
  `subscription_entities` 0 -> 1. Reverted; a re-sync is byte-identical to the
  baseline again.

Images, built and run on this host: `just esp32 build-qemu` (both flash images,
stack-floor check OK) and `test_esp32_qemu_talker_boots` +
`test_esp32_talker_listener_e2e` PASS; `fixtures-build.sh baremetal rust` (every
`mps2-an385-baremetal` row) and `test_qemu_bsp_pubsub_e2e` +
`test_qemu_serial_pubsub_e2e` PASS on the derived pools. The baremetal and esp32
scaffolds both cross-compile after `nros sync`.
`test_qemu_xrce_pubsub_e2e` FAILS -- "no RMW backend is registered" -- for a
reason on `origin/main` that this change does not touch: **issue 1601**.

### Not done here, filed

* **Issue 1603** -- 32 other cross-only Rust leaves still reach no probe (8 RTIC,
  6 Zephyr, and 18 FreeRTOS/NuttX/ThreadX leaves whose result in this worktree
  was "vendored source not provisioned", i.e. unmeasured). None of them declares,
  so none of them was what this issue owned; they are the same class.
* **Issue 1602** -- on the probe road the descriptor's subscription rows name the
  callback (`on_chatter`) in `topic`; it was `/chatter` while the esp32 listener
  declared.
* The twelve NuttX C/C++ leaves (issue 1556) are now the ONLY leaves that
  declare entities by hand.
