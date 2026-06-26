---
id: 101
title: "Per-board boot config (node_name + locator + domain) resolves 4 different ways — not unified"
status: open
type: tech-debt
area: core
related: [phase-244, rfc-0004, rfc-0014, rfc-0045]
---

> **Design: [RFC-0045](../design/0045-unified-boot-config-resolution.md)** (Draft, 2026-06-26)
> — the unified resolver, precedence model A, `BootConfig`/`ExecutorConfig::resolve` placement,
> and the `.nros_boot_config` single bake site. Read it for the rationale; this issue tracks the
> work.

## Status (2026-06-27)

**Rust core: RESOLVED** ([phase-266](../roadmap/phase-266-unified-boot-config-resolution.md)
W1–W4, landed `…a314b02eb`). One resolver (`ExecutorConfig::resolve`, precedence A) now owns
node_name; every Rust board (hosted, OwnedSpin, NuttX +`run_with_deploy`, RTIC, Embassy)
resolves it from the single `.nros_boot_config` bake site instead of the former four ad-hoc
mechanisms. NuttX gained the missing `run_with_deploy`. `just check` green + whole-branch review.

**Still open:** (1) **C/C++ session naming** — `nros_support_init_named` / `nros_cpp_init` /
codegen-entry don't yet pass the configured name (phase-266 W5/W6); (2) **locator/domain** are
threaded but, on hosted, deliberately stay env-authoritative (the resolver maps node_name; full
overlay-authority for locator/domain is a follow-up); (3) **cleanup** — merge the two
`board_path_for` maps + resolve the near-dead `setup_transport` (phase-266 W7). This issue stays
`open` until W5–W7 land.

## Summary

How a board obtains its boot `ExecutorConfig` — **node name, locator, domain id** — is
ad-hoc per board family. A 2026-06-26 audit of every Rust board found **four** different
resolution mechanisms with no unified override story, so the same `system.toml` /
`[deploy.*]` / launch input produces different runtime identity depending on the target.

| Mechanism | Boards | node_name | locator | domain |
| --- | --- | --- | --- | --- |
| Runtime env (`from_env`) + `DeployOverlay` | `PosixBoard`, `NativeBoard` | `deploy.node_name` ✅ | `NROS_LOCATOR` | `ROS_DOMAIN_ID` |
| Deploy overlay baked into a `Config` | `stm32f4`, `mps2-an385` (bare/FreeRTOS), `threadx` (linux/riscv) | hardcoded `"nros_app"` ❌ | overlay/compile | overlay/compile |
| Compile-time `option_env!` | Zephyr, ESP32, RTIC, Embassy | `option_env!`/hardcoded ❌ | `option_env!("NROS_LOCATOR")` | `option_env!("NROS_DOMAIN_ID")` |
| **Default trait body (overlay dropped)** | **NuttX** (`qemu-arm`, `qemu-riscv`) | inert ❌ | inert | inert |

## Concrete defects this causes

1. **node_name (issue #98) only works on 2/10 boards.** The macro bakes
   `DeployOverlay.node_name` for every OwnedSpin/Zephyr/ESP32 target, but only
   `PosixBoard`/`NativeBoard` apply it (`nros-board-posix/src/lib.rs:199-201`). Bare-metal
   boards hardcode `.node_name("nros_app")` (`nros-board-stm32f4/src/entry.rs:115`,
   `nros-board-mps2-an385/src/entry.rs:138`); RTIC/Embassy use compile-time
   `option_env!("NROS_NODE_NAME")` (`nros-board-rtic-stm32f4/src/lib.rs:476`).

2. **NuttX has no `run_with_deploy` override** (`nros-board-nuttx-qemu-arm/src/entry_212n.rs`,
   `...riscv/...`). The trait default forwards to `run` and drops the overlay entirely, so
   `[package.metadata.nros.deploy.nuttx*]` (locator, ip, domain_id, node_name) is silently
   inert. Parity-broken vs FreeRTOS/ThreadX, which do override it.

3. **Locator/domain override stories diverge.** Hosted POSIX honors runtime env (overridable
   per invocation); bare-metal/Zephyr/RTIC/Embassy bake at compile time (`option_env!` or
   deploy metadata). A developer must know which board uses which knob — no single answer.

4. **`setup_transport` is dead on 9/10 boards** — only `mps2-an385` (xrce-transport feature)
   overrides it, yet `nros::main!` always emits the call. Either a template for future
   custom transports (document it) or remove it.

5. **Two board-key→path maps in Rust** that can drift: `main_macro::board_path_for` (the
   `nros::main!` path) and `codegen/entry/emit_rust.rs::board_path_for` (the `nros codegen
   entry --lang rust` path). The latter lacks `freertos` and silently falls back to
   `NativeBoard`.

## Fix direction

Make `DeployOverlay` the single boot-config channel, applied uniformly **before
`Executor::open` on every board**:
- Every `BoardEntry` impl overrides `run_with_deploy` (NuttX first — it has none) and threads
  `overlay.node_name` / `overlay.locator` / `overlay.domain_id` onto the `ExecutorConfig`,
  falling back to the board default (`"nros_app"`, compiled locator) only when the overlay
  field is `None`. Mirror `nros-board-posix/src/lib.rs:199-201` everywhere.
- For RTIC/Embassy, thread the overlay into `init_hardware_with_deploy` (RTIC already takes
  `&DeployOverlay` as of Phase 244.D1; Embassy needs the same) so per-deployment identity is
  possible without compile-time env.
- Decide the locator/domain override contract: keep env on hosted (dev flexibility) but make
  the overlay authoritative when set, on ALL targets. Document the precedence once.
- Merge the two `board_path_for` maps into one.
- Resolve `setup_transport` (keep+document as the custom-transport seam, or remove).

Closing this makes **#98** a special case (node_name is just one overlay field) and removes
the embedded config divergence flagged across the board audit.

## Evidence

Found 2026-06-26 in a per-board unification audit while extending the #98 node-naming fix.
Per-board `BoardEntry` override table + file:line refs are in that audit; the headline is
that only `PosixBoard`/`NativeBoard` honor the launch-driven boot config the macro already
bakes for every board.
