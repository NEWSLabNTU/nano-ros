# Phase 264 — mature `nros::main!` to wire the full system.toml capability set

Status: **Planned (2026-06-20)** · Resolves [issue 0089](../issues/0089-declarative-node-pkg-api-gaps-block-workspace-feature-demos.md)
· Unblocks phase-263 A2 / A3 / A5 + the showcase halves · RFC-0015, RFC-0024, RFC-0032.

> **Goal.** Make the plain-cargo workspace Entry (`nros::main!`) honour the same
> `system.toml`-declared capabilities the `codegen-system` **bake** already lowers, so
> the starter/showcase workspaces can demonstrate lifecycle, logging, and parameters —
> not just pub/sub, timer, services, tiers, and safety.

## Why (the 0089 finding, 2026-06-20)

Implementing phase-263 A1–B2 proved `nros::main!` is a **thin** macro: it emits one
`register` per launch `<node>` and **resolves `[tiers]`** (`main_macro.rs` imports
`resolve_tiers` → `run_tiers`), but does **not** wire the rest of the `system.toml`
capability set. Those are honoured only by the bake (`generate.rs`) or the `[[bin]]`
executor shape. So the workspace (cargo) build mode gates:

| Capability | Bake (`generate.rs`) | `nros::main!` today |
| --- | --- | --- |
| nodes (`register`) | ✓ | ✓ |
| scheduling tiers | ✓ | ✓ |
| `[lifecycle]` (REP-2002 services + autostart) | ✓ `render_lifecycle_fn` | ✗ |
| log-sink init | (board/runtime) | ✗ — node logs vanish |
| `[param_services]` + parameter values | ✓ | ✗ |
| safety (CRC) | ✓ (features) | ✓ *only if the entry sets the cargo features by hand* |

`generate.rs` is the reference implementation for every row; this phase ports the
missing rows into `main_macro.rs` (+ the small core/board hooks they need).

## Cross-cutting decision — the cargo-feature-flow wrinkle

The bake **writes** the generated Cargo.toml, so it adds `nros/lifecycle-services`,
`nros/param-services`, `nros/safety-e2e` as needed. `nros::main!` **cannot** edit the
(user-authored) Entry Cargo.toml, and macro-emitted `#[cfg(feature = …)]` resolves
against the ENTRY crate's features, not `nros`'s. Options (decide in W1):

- **(a) Entry opts in, macro emits unconditionally.** The Entry Cargo.toml enables the
  needed `nros` features when its `system.toml` declares the capability (as B1 does for
  safety today); `nros::main!` emits the registration unconditionally and fails loud if
  the feature is missing. Simplest; one manual line per capability in the Entry.
- **(b) Board carries forwarding features.** The board crate exposes
  `lifecycle-services` / `param-services` features (like `nros-board-native/safety-e2e`)
  that forward to `nros`; the Entry enables them on the board dep. Mirrors the safety
  precedent; keeps the Entry's `nros` dep clean.
- **(c) `nros ws sync` injects them.** Teach `ws sync` to read `system.toml` and set the
  Entry's `nros`/board features in the managed block — fully automatic, no manual line.
  Most magic, most moving parts.

Recommended: **(b)** for the runtime-service features (consistent with safety) + **(c)**
as a follow-up for zero-touch. Lock in W1.

## Work items

### W1 — decision + the feature-flow mechanism — RESOLVED (2026-06-20, exploration)
**Finding: lifecycle/params need NO board forwarding.** `lifecycle-services` and
`param-services` are features on the **`nros` umbrella** (`nros/lifecycle-services` →
`nros-node/lifecycle-services`, `nros/Cargo.toml:128/131`), which the Entry pkg
already deps directly — so **option (a)**: the Entry adds the feature to its own `nros`
dep (one line, like B1's safety), cargo unifies `nros-node` across the graph, the
service method exists, the macro emits the call unconditionally. Only **safety** needs
board forwarding (the CRC lives in the backend the board owns — already done). So W1 is
a convention, not code: document "declare `[lifecycle]`/`[param_services]` → add the
matching `nros` feature to the Entry" (until W-future `ws sync` auto-injects it).

### W2 — lifecycle in `nros::main!`
**Integration point (found 2026-06-20):** the macro's `register` closure gets a
`RuntimeCtx<'a>` (`nros-platform/src/board/runtime.rs:159`), which exposes `param` /
`remap` / `env_var` but **not** `register_lifecycle_services` (that's on
`Executor`, `nros-node`). So W2 = **(i)** add a `RuntimeCtx` forward —
`register_lifecycle_services()` + `lifecycle_state_machine_mut()` — to the underlying
`NodeDispatchRuntime`/executor (cfg `lifecycle-services`); **(ii)** `main_macro.rs`
reads `[lifecycle]` from `system.toml` (it already opens it for
`default_launch`/`[tiers]`) and, when present, emits the registration + autostart
transitions into the run closure, mirroring `generate.rs::render_lifecycle_fn` exactly
(`register_lifecycle_services()?` then `trigger_transition(Configure[/Activate])`).
Unblocks **phase-263 A3**. Test: a workspace fixture with `[lifecycle] autostart`
builds + registers the 5 services.

**W2 — IMPLEMENTED (2026-06-20).** Added: `NodeDispatchRuntime::apply_lifecycle(
autostart: u8)` (nros-platform, default no-op) + `RuntimeCtx::apply_lifecycle` forward;
`ExecutorNodeRuntime::apply_lifecycle` override (`nros`, `#[cfg(lifecycle-services)]` →
`register_lifecycle_services()` + `trigger_transition(Configure[/Activate])`, mirroring
`render_lifecycle_fn`); `nros::main!` reads `[lifecycle]` (`read_lifecycle_autostart`)
and emits `runtime.apply_lifecycle(code)?` after the `register` calls at both the
single-tier and `run_tiers` sites. **W2 VERIFIED (2026-06-20)** via
`examples/workspaces/ws-lifecycle-rust` — a plain-cargo entry with `[lifecycle]
autostart = "active"` + `nros/lifecycle-services`: `cargo build -p native_entry` links
clean (14.5s), so the macro reads `[lifecycle]`, emits `runtime.apply_lifecycle(2)`, and
the override registers the 5 services + drives Configure→Activate. **This IS phase-263
A3** (first plain-cargo workspace lifecycle demo). The earlier bare-`nros-node` failure
(0092) was a backend-less build — a real entry has an RMW backend → `has_rmw` →
`lifecycle-services` compiles; **0092 downgraded** to a minor robustness gap
(lifecycle-services should gate-or-imply `has_rmw`).

### W3 — log-sink init at boot
A node's `nros_info!` needs a registered sink. The board is the only layer that knows
its sink (native → stdout; embedded → its writer). Add a `BoardPrint`/`BoardEntry`
hook (or a default-sinks associated const) so the runtime calls
`nros_log::init(<board>::default_log_sinks())` once at boot — opt-out for size-bound
embedded. `nros-board-native` delegates to `nros-board-posix`; the init lands in the
posix runtime. Unblocks **phase-263 A5**. Test: a node logging in a callback produces
output under the native entry.

**W3 DONE (2026-06-20).** `nros-board-posix` now deps `nros-log` and calls
`nros_log::init(nros_log::sinks::default())` at the top of both `run` and `run_tiers`
(idempotent), so the default platform sink (host → stdout/stderr) is live before the
user closure — a Node pkg's `nros_info!` reaches output with no per-app init. Builds
clean. Used `sinks::default()` (the existing `PlatformSink`) rather than a new board
hook — simpler, and the posix `PlatformSink` already routes to the host. (Embedded
boards keep their own writers; this lands only in the posix family driver, so no
size-bound target is affected.) Unblocks phase-263 A5 (a logging node now produces
output; the A5 workspace demo + runtime assert is the phase-263 deliverable).

### W4 — parameters (NOT STARTED — largest item; the resume point)

Design SSoT: **RFC-0004 §10 (Runtime parameters)**. Summary (maintainer model, 2026-06-20):

1. **Initial values are COMPILE-BAKED from the launch file.** `nros::main!` already reads
   the launch XML at expansion time; it bakes each `<param name=… value=…/>` as the
   node's **initial** parameter value (a compile-time constant in the generated entry),
   consistent with the declarative baking model — NOT a runtime launch-string lookup.
2. **Runtime reconfiguration is VOLATILE (RAM).** A param store seeded from the baked
   initials; `declare_parameter` registers, `ctx.parameter::<T>(name)` reads, and
   `[param_services]` (`ros2 param get/set`) updates the RAM store — **values live until
   the next boot**.
3. **Persistence is OUT OF SCOPE** — flash/NVS backing needs consistent storage (the
   dormant `nros-params` `ParamStore` backends, **issue 0080**). Deferred.

Sub-waves:

- **W4a — bake initials + typed read.** `nros::main!` (+ `generate.rs` for the bake
  path) emits each declared parameter's baked initial value into the generated entry +
  seeds a volatile param store; add `CallbackCtx`/`TickCtx::parameter::<T>(name) ->
  Option<T>` reading that store (baked initial, until reconfigured). A node `declare`s a
  parameter and reads its launch-set value in a callback. Unblocks the read half of
  **phase-263 A2**. Test: a workspace node declares + reads a param; changing the launch
  `<param value=…/>` changes the baked initial (rebuild) and the read value.
- **W4b — `[param_services]` runtime reconfig.** Register the ROS 2 parameter services
  (same W2 macro mechanism) so `ros2 param get/set` reads/updates the RAM store live
  (until reboot). Unblocks the reconfig half of A2. Test: `ros2 param set` changes the
  value a running node reads.

**No persistence layer** (W4 explicitly excludes flash/NVS — issue 0080). New API spans
`nros-params` (activate the dormant volatile store) + `node`/`node_runtime`
(`CallbackCtx::parameter`) + the macro (bake + seed). Keep `generate.rs` and
`main_macro.rs` aligned so the bake and cargo paths bake identically.

## Sequencing
W1 (feature mechanism) → W2 (lifecycle — smallest macro change) → W3 (log-init —
board) → W4 (parameters — core API, largest). Each ships independently + reopens its
phase-263 wave. Keep `generate.rs` and `main_macro.rs` behaviourally aligned (a shared
helper where practical) so the bake and cargo paths can't drift.

## Acceptance
- A workspace built with `nros::main!` (not the bake) demonstrates lifecycle, logging,
  and parameters — closing phase-263 A2/A3/A5 in the cargo shape.
- The bake and `nros::main!` produce equivalent runtime behaviour for the same
  `system.toml` (no cargo-vs-bake capability gap).
- Issue 0089 resolved.

## Notes
- This is the prerequisite the phase-263 re-sequence (2026-06-20) gated A2/A3/A5 behind.
  B1 (safety) + B2 (tiers) already shipped because they needed no macro change.
- Don't fold log-init into every board blindly — gate it so size-bound embedded targets
  opt out (W3).
