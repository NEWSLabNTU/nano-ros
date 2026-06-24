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

### W4 — parameters (W4a + W4b + W4c DONE 2026-06-20)

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

- **W4a — bake initials + register-time read. IMPLEMENTED + VERIFIED (2026-06-20).**
  `nros::main!` now compile-bakes each launch `<param name=… value=…/>` into the
  generated entry and seeds it into the node's `NodeContext` before the `register` call;
  the node reads its launch-set value with `ctx.param(name) -> Option<&str>`. Test:
  `examples/workspaces/ws-params-rust` — `ParamTalker::register` reads
  `ctx.param("publish_period_ms")` and drives its publish-timer period from it;
  `cargo build -p native_entry` links clean (~15s) and the nightly-expanded entry shows
  `runtime.params = &[("publish_period_ms", "250")]; ::param_talker_pkg::register(runtime)?;`
  — the launch value flows into the node with **no per-app glue and no extra `nros`
  feature**. Closes the read half of **phase-263 A2** for the macro path.

  **Implementation:**
  - `node.rs` — `NodeContext` gained a `params: &[(&str,&str)]` field + `set_params` /
    `param(name)`.
  - `node_runtime.rs` — `register_node_borrowed` takes a `params` slice and calls
    `context.set_params(params)` before `C::register`; new
    `install_node_typed_with_params<C>(executor, params)` (the existing
    `install_node_typed` forwards with `&[]`); stubs mirrored in `lib.rs`.
  - `nros-macros` — the `nros::node!` `register` wrapper calls
    `install_node_typed_with_params::<T>(executor, runtime.params)`; `main_macro.rs`
    bakes per-node launch `<param>` as a promoted `&'static` slice and emits
    `runtime.params = &[…];` before each `::<pkg>::register(runtime)?;` (reset to `&[]`
    for nodes with none, so params never leak between registrations).
  - `RuntimeCtx::params` (`nros-platform`) was already the carrier — no board change.

  **Bake-path alignment (deliberate, recorded for W4b):** the `nros::main!` path reads
  baked initials at register via the new `NodeContext::param`; the **bake** path
  (`generate.rs::register_all` → `instantiate_components`) does **not** use this seam —
  it has its own parameter system (`apply_param_persistence` + `[param_services]` +
  `declare_parameter`, a runtime store). So the two paths bake initials by different
  mechanisms today. They **converge in W4b**, where the macro path adopts the same
  volatile `nros-params` store the bake uses (the `CallbackCtx::parameter::<T>` typed
  read + `ros2 param set` reconfig). W4a deliberately ships the minimal register-time
  string read first (no store), since reconfig — the only consumer that needs the store
  — lands in W4b.
- **W4b — volatile store + `[param_services]` registration. IMPLEMENTED + VERIFIED
  (2026-06-20).** `nros::main!` now reads `[param_services]` from `system.toml` and, when
  present, emits `runtime.apply_param_services(&[…baked initials…])` after the per-node
  `register` calls — registering the 6 ROS 2 parameter services on the executor and
  seeding a volatile `nros-params` store from the aggregate launch `<param>` initials
  (raw launch strings; the runtime infers each `ParameterValue` type). So `ros2 param
  list/get/set` works against the running macro-path node; reconfigured values live in
  RAM until the next boot. **This adopts the same `register_parameter_services()` +
  `declare_parameter()` executor seam the bake's `apply_param_persistence` drives — the
  bake/macro convergence point** (the macro path now reaches the store the bake already
  used). Test: `examples/workspaces/ws-params-rust` declares `[param_services]` + enables
  `nros/param-services`; `cargo build -p native_entry` links clean and the nightly-expanded
  entry shows, after the register call:
  `runtime.apply_param_services(&[("publish_period_ms", "250")]).map_err(…)?;`.

  **Implementation (mirrors W2 lifecycle exactly):**
  - `nros-platform` — `NodeDispatchRuntime::apply_param_services(params)` (default no-op)
    + `RuntimeCtx::apply_param_services` forward.
  - `nros` — `ExecutorNodeRuntime::apply_param_services` override (`#[cfg(param-services)]`)
    → `register_parameter_services()` then `declare_parameter(name, infer_param_value(raw))`
    per baked entry; `infer_param_value` maps `true`/`false`→Bool, `i64`→Integer,
    `f64`→Double, else String (the type a `ros2 param set` of the same literal lands on).
  - `nros-macros` — `read_param_services_enabled(system.toml)` (`[param_services]` present)
    + a `param_services_call` token stream (aggregate of every node's baked `<param>`)
    spliced after `#lifecycle_call` at both the `run` and `run_tiers` sites.

  **Deferred to W4c** — the *in-node* typed read `CallbackCtx::parameter::<T>(name)`. The
  callback trampoline (`dispatch_into_cell`, `node_runtime.rs`) holds only the
  `ComponentCell` + payload — it has **no reach to the executor's `ParamState`** (the
  store lives in the executor; the leaked `*Ctx` trampolines don't carry it). So a node
  observing a `ros2 param set` mid-run needs the store threaded into `CallbackCtx`, a
  larger change than W4b's registration+seed. W4b ships the full `ros2 param get/set`
  reconfig surface (store + services + CLI interop); W4c adds the in-node read.

- **W4c — `CallbackCtx`/`TickCtx::parameter::<T>` in-node read. IMPLEMENTED + VERIFIED
  (2026-06-20).** A node reads the live (baked-or-reconfigured) value in `on_callback`
  **and** `tick` via `ctx.parameter::<T>(name) -> Option<T>`. **Runtime-verified** over the
  wire: `tests/param_live_read_e2e.rs` boots `ws-params-rust` + an nros subscriber and
  asserts the node publishes `250` — the launch-baked initial read LIVE each tick via
  `ctx.parameter::<i64>("publish_period_ms")` (passes in-env, nros↔nros). The `ros2 param
  set publish_period_ms 500 → node publishes 500` reconfig path is
  `tests/params.rs::test_ros2_param_set_reconfigures_live_read` (ROS 2 interop lane; skips
  where the distro `rmw_zenoh_cpp` mismatches the pinned zenoh wire version — needs `just
  rmw_zenoh setup`). Plus a unit test (`node.rs::callback_ctx_reads_param`).

  **Implementation (as designed):**
  - `ComponentCell` gained `param_server: Cell<*const ParameterServer>` (cfg
    `param-services`) + accessor; `register_node_borrowed` / `register_node` capture the
    store address on the cell after registration.
  - `nros::main!` emits `apply_param_services` **before** the per-node registers (reorder
    from W4b) so the store exists when each cell captures it.
  - `dispatch_into_cell` + the safety-dispatch + the 3 with-reply/goal/cancel trampolines
    thread `cell.param_server()` onto `CallbackCtx::set_param_server`; the action
    result/feedback/accepted trampolines route through `dispatch_into_cell` already.
  - `tick_one_cell` threads it onto `TickCtx` (via the cell, NOT `exec_ptr` — the store is
    a separate `Box<ParamState>` allocation, so it can't alias the `&mut Executor` the
    tick's client/action calls reborrow).
  - `CallbackCtx`/`TickCtx` gained the `params` field + `set_param_server` +
    `parameter::<T>` (reads `ParameterServer::get` → `ParameterVariant::from_parameter_value`).
  - Test infra: `[[workspace_fixture]] workspace-rust-native-params` (cargo `nros::main!`
    path — `workspace-fixtures-build.sh` now skips `codegen-system` when `codegen_out` is
    absent) + `build_native_workspace_rust_params_entry()`.

  **Borrow safety:** single-threaded executor; param services mutate the server only
  before/after dispatch (`spin.rs:4143`/`:4531`), never during; `Box<ParamState>` keeps
  the address stable; the server's fixed `[Option<_>; MAX]` array never reallocs on
  declare. The `*const → &` deref in `ComponentCell::param_server` is sound under those
  invariants.

  ~~Thread the executor's volatile param store into the dispatch paths.~~ (Original
  design below, kept for reference.)

  **Read API (both ctxs):** `parameter<T: nros_params::ParameterVariant>(&self, name) ->
  Option<T>` = `self.params.and_then(|s| s.get(name)).and_then(T::from_parameter_value)`.
  `ParameterVariant` (bool/i64/f64/String) + `ParameterServer::get` already exist.

  **Store reach — split by dispatch family (the design crux):**
  - **TickCtx** — free: `tick_one_cell` (`node_runtime.rs:383`) already holds
    `exec_ptr: *mut Executor` (the existing disjoint-borrow precedent), so
    `(*exec_ptr).params()` flows straight in.
  - **CallbackCtx** — the real work: the arena subscription/timer closures + the 6 leaked
    service/action `*Ctx` trampolines capture only `Arc<ComponentCell>` — the executor is
    **not** reachable when they fire (closure built at registration, invoked deep in
    `arena.rs`). So the store must arrive via the cell. (NB: passing
    `executor.params()` at the `dispatch_into_cell` *call site* does NOT work — the
    closure has no `self`.)

  **Mechanism (no emit reorder):**
  1. `ComponentCell` gains `param_server: Cell<*const ParameterServer>` (cfg
     `param-services`, null default) + `set_param_server`/`param_server` accessors.
  2. `ExecutorNodeRuntime::apply_param_services` (W4b; runs after the registers) — after
     `register_parameter_services` + seed, a **post-pass** sets the ptr on every
     `self.components` cell, mirroring `run_ticks`: `let p = self.executor.params() as
     *const _; for cell in &self.components { cell.set_param_server(p); }` (disjoint
     `&self.components` / `&mut self.executor`; take address, drop borrow).
  3. `dispatch_into_cell` + the 6 trampolines: after building `ctx`, if the cell ptr is
     non-null, `ctx.set_param_server(Some(unsafe { &*ptr }))`.

  **Borrow safety (justifies the `unsafe` deref):** single-threaded executor; within
  `spin_once`, param-services mutate the server at `spin.rs:4143` (pre) and `:4531`
  (post) with all dispatch *between* — no overlapping `&mut`/`&`. `Box<ParamState>` →
  stable address; `ParameterServer` is a fixed `[Option<_>; MAX]` array → declare never
  reallocs; ptr valid for the executor's life.

  **Blast radius** (all cfg `param-services`): ComponentCell +1 field/+2 methods;
  `apply_param_services` + post-pass; `CallbackCtx` +1 field (None in its 5 constructors)
  + `set_param_server` + `parameter::<T>` (mirrors W4a `NodeContext::set_params`);
  `dispatch_into_cell` + 6 trampolines +1 line each; `TickCtx` +1 field/method +
  `tick_one_cell` wire.

  **Scope decisions (2026-06-20):** implement on **both** `CallbackCtx` + `TickCtx`;
  verify build-tier (expand) + a unit test (seed store → dispatch → assert
  `ctx.parameter`) **and** a runtime E2E (`ros2 param set` → the running node observes the
  new value via `ctx.parameter::<T>`). Extend `ws-params-rust`'s node to read
  `ctx.parameter::<i64>("publish_period_ms")` in its callback. Known limitation
  (unchanged): one param server, registered under the executor default node name —
  multi-node per-node param scoping is out of scope.

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
