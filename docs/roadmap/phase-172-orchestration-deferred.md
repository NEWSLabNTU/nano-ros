# Phase 172 — Orchestration follow-ups (deferred from Phase 126)

**Goal.** Land the capabilities that Phase 126 (ROS 2 workflow
orchestration MVP) explicitly deferred, **plus the configuration
consolidation absorbed from Phase 116** (archived 2026-05-27). Phase
126 shipped the end-to-end MVP — source metadata → launch plan →
checked `nros-plan.json` → generated per-board binary, verified across
9 boards. This phase organizes the remaining work into **four parallel
work groups** (see below).

**Status (2026-05-28).** Groups 1–4 = **completed planning foundation** (L, M,
J, K.1–K.6, N, B, C, G, A, H, I, D, F). **Group 5 (revised deployment model)
is largely landed** and remains the single active direction. Done: the
two-form entry lib (WP-B — compiled `.a`+header / source crate+CMake, C ABI,
config override, RMW-set check, multi-domain session opening); the root
`nros.toml` SSOT + `nros deploy` command-runner + scaffolder + doctor pin-check
(WP-A); **the flip** — entry lib generalized to *every* non-bridge platform,
`render_main`/`EntryKind`-main emitter deleted, dead `SystemConfig` retired; and
**all three "no-compat" replacements have executed** — the generated-`main` path
(→ entry lib), the flag-driven `nros build --launch/--system-plan` (→ `nros
build|deploy <name>`, retired `01c8512`), and the per-package
`system nros.toml` triple/board reader (retired) are *deleted*, not kept
alongside. The **`self` model is proven end-to-end on QEMU/native**
(`nros deploy native` builds + boots from one root file).

**Remaining (the gap between "designed/structural" and "proven"):**
- **W.1 — DONE** (2026-05-28, codegen `1ff8a51`+`685354e`). Cargo-style manifest
  fold landed: a package's `[component]` table folds into its `nros.toml`
  (`workspace::load_component_config` reads either the folded table or the
  legacy standalone `component_nros.toml`, which now warns once); `ManifestKind`
  + `probe_manifest_kind` discriminate by sections present (`[workspace]`
  dominates); `resolve_workspace_root` walks up to the nearest enclosing
  `[workspace]`, so `nros deploy`/`build <name>` work from any member dir, and a
  non-workspace manifest with no enclosing workspace gives a kind-specific
  direct-mode/component error.
- **W.3 — DONE** (codegen `d5d6382`+`f1fc4f4`+`51cb0d0`). `[overrides]` + its
  `parameters`/`remaps` and the whole `[linkage]` table are `#[serde(default)]`
  (a minimal `[component]` = package + component + language + metadata parses);
  `metadata --build` derives executable / exported-symbol / crate_name from the
  component name + crate convention (`ComponentLinkage::resolved_*`); and `nros
  new --component <name>` scaffolds a planned-mode component (an `nros::Component`
  lib + a folded minimal `[component]` `nros.toml`), so `nros metadata --build`
  records it with zero hand-edited linkage. (The direct-mode binary scaffold
  stays a `[node]` manifest — `--component` is the orchestration counterpart.)
- **W.4** — one real vendor-lib + one real vendor-module build.
  - **vendor-module: DONE** (2026-05-28, codegen `b606dae`). `nros deploy
    zephyr-mod` drives a real `west` cross-build (generate → west → cmake/ninja
    → native_sim `zephyr.exe`, [1245/1245]); boots + runs the nros entry. Fixes:
    deploy absolutizes the workspace root; Zephyr is no_std (the native_sim host
    triple no longer forces `std`); prj.conf bakes per-RMW Kconfig
    (`CONFIG_POSIX_API` ⇒ fixes the zenoh-pico-zephyr C header clashes); the
    Zephyr `rust_main` references the `zephyr` crate (allocator/panic).
    `deploy_zephyr_vendor_module_real_west_build` e2e (gated on `ZEPHYR_BASE`).
    Networking uses **NSOS** (host BSD sockets via the deploy `self` glue's
    `native-sim-nsos.conf` + `EXTRA_CONF_FILE`) — no TAP/root; boot reports
    `Network ready (NSOS)`. `[deploy].locator` bakes the agent address
    (`TRANSPORT_LOCATOR`) so the embedded app connects (verified: boots →
    NSOS → reaches the transport-connect stage on the baked
    `tcp/127.0.0.1:7456`).
    *A full Published/Received demo is blocked, not on the deploy/transport
    path, but on an unimplemented architectural feature — see **W.5** below.*
  - **vendor-lib: pipeline now proven on host** (172.V). The full
    `dry_run:false` emit→link→package path is exercised by
    `deploy_vendor_lib_real_build_with_stub_lib` — a `[deploy.orin-stub]`
    target (x86_64, `emit=compiled`) emits the compiled entry lib
    (`libnros_orin_stub.a`) and links it + a vendor `startup.c` against a stub
    `libfakevendor.a` the test builds, producing an ELF — no license-gated SDK.
    Only the *actual NVIDIA SPE FSP* link (`NV_SPE_FSP_DIR`,
    `libtegra_aon_fsp.a`) stays gated (template + dry-run also landed); unblocks
    when the maintainer installs SDK Manager.
- **W.5 — component callback-body execution** (NEW, found 2026-05-28). The
  orchestration component model is **declarative-only**: `Component::register`
  declares the graph (nodes / publishers / timers / `CallbackId`s + effect
  metadata for planning + scheduling), and `create_timer`/`create_subscription`
  take only a `CallbackId`, **no closure**. The generated runtime wires each
  callback as a noop `|| {}`; the `ComponentPublisher` returned by `register`
  is zero-sized (no handle to publish through). So a deployed orchestration
  binary registers the graph + connects to the agent, but **emits no data** —
  no planned component can carry executable callback logic. Closing this needs
  a real feature: bind callback bodies to declared `CallbackId`s (e.g. a
  `Component::on_callback(CallbackId, &mut CallbackCtx)` that resolves the
  runtime publisher by `EntityId` — needs an executor publisher-registry +
  component API + codegen change), after which `demo_pkg` (or any component)
  can actually publish and the zephyr-mod deploy proves the data-plane end to
  end. Deferred (own design + impl; orthogonal to the W.4 deploy/transport
  path, which is done).

  **Design + slices (scoped 2026-05-29; unblocks [189.M3.5]).** The executable
  layer sits *over* the declarative `Component::register` (which stays the
  planning/metadata SSOT). **Decision (LOCKED 2026-05-29): immediate publish —
  no deferred queue.** The re-entrancy worry was unfounded: `EmbeddedRawPublisher`
  is a **self-contained transport handle** (`publish_raw(&self)` holds its own
  `RmwPublisher` and writes straight to the transport — it never touches the
  `Executor`, `handles.rs:683`). So a callback can publish *immediately* through
  a captured/owned publisher while the executor is mid-`spin_once`. A deferred
  queue was rejected because it breaks causality — a later callback could not
  observe that an earlier one's publish had happened.
  - **Publish path:** each callback closure holds its `EmbeddedRawPublisher`(s)
    (the `publishes` effect names them); `publish_raw(&self)` is immediate +
    causal (wire ordering preserved; local loopback stays next-I/O-cycle, as in
    stock ROS).
  - **Shared component state, `no_std`:** `core::cell::RefCell<State>` —
    **`RefCell` is in `core`, no `alloc`/`Rc`.** The generated runtime owns the
    `State` as a single `'static` instance (one generated binary = one deployed
    component graph), captured `&'static RefCell<State>` into each callback
    closure (closures stay `'static`, which the arena requires); shared mutation
    via `borrow_mut()`. Publishers likewise live in `'static` storage (moved into
    their callback's closure, or shared by-ref when several callbacks publish the
    same entity).
  Slices:
  - **W.5.1 — `CallbackCtx` + dispatch surface. DONE** (2026-05-29,
    `nros/src/component.rs`). `PublisherResolver` (generated runtime impls it),
    `CallbackCtx<'a>` { `payload()`, `message::<M>()` (deserialize), `publish_raw`,
    `publish::<M, N>` }, and `trait ExecutableComponent: Component { type State;
    fn init() -> State; fn on_callback(&mut State, CallbackId, &mut CallbackCtx) }`
    — trait-dispatch, no `alloc`. Test
    `executable_component_callback_publishes_and_mutates_state` proves a body
    mutates state + publishes through the resolver. (Pure API substrate; no
    executor/codegen wiring yet — W.5.2/.3.)
  - **W.5.2 — publisher wiring + generated `PublisherResolver`.** Concrete design
    (mapped 2026-05-29 against `generate.rs`):
    - **Publishers are not created today.** `Component::register` records publisher
      *metadata* only; the generated runtime never builds a real publisher for them
      (`ComponentPublisher` is zero-sized; the callback loop only registers
      sub/timer/service/action). So W.5.2 must *emit publisher creation*: for each
      `PlanEntity::Publisher`, generate a `node.publisher(topic).generic(type,hash)
      .build()` → owned `EmbeddedRawPublisher`, kept in `'static` storage.
    - **Bespoke resolver, not a generic registry.** The generator emits a per-binary
      struct holding the publishers as fields + an `impl PublisherResolver` whose
      `publish_raw(entity_id, data)` matches `entity_id` → the right field's
      `publish_raw`. (Simpler for codegen than a runtime registry type; no nros-side
      addition.)
    - **`'static` State init (the open mechanism decision).** `State` comes from
      non-const `<C as ExecutableComponent>::init()` but closures need `'static`.
      no_std/no-alloc options: `static mut STATE: Option<RefCell<State>>` + unsafe
      init in `register_all` (single-threaded executor ⇒ sound), or a `spin::Once`.
      **Decide before coding.** Publishers stored the same way.
  - **W.5.3 — codegen (closes [189.M3.5]).** Emit, per callback-bearing entity
    (sub/timer/**service/action**), a closure/trampoline that builds `CallbackCtx`
    (payload + the `&PublisherResolver`) and calls `<C as ExecutableComponent>::
    on_callback(&mut state, CallbackId::new(id), &mut ctx)` — replacing the noop
    `||{}` / `noop_raw_*`. **Executable-detection — RESOLVED 2026-05-29:** there is
    *no* `component!`-body macro to extend (`nros::component!` is the export-symbol
    proc-macro; components impl the trait directly). So: **instantiated components
    must impl `ExecutableComponent`** and the generator emits `on_callback`
    unconditionally; a `nros::declarative_component!(Ty)` helper emits a noop
    `ExecutableComponent` (`State = ()`) for body-less components so declarative-only
    ones still compile. (b) services/actions take raw C-fn-ptr callbacks (not
    closures), so their trampoline reads the `static` state + resolver via the
    generated globals rather than a captured closure. **All-or-nothing for
    compilation** (state + publishers + resolver + closures + the component's
    `ExecutableComponent` impl + the generated Cargo dep land together); verify via
    `orchestration_generate` source-asserts, then generate→`cargo check`→run (W.5.4).
    *(Status 2026-05-29: W.5.1 substrate + `declarative_component!` macro + `nros::`
    re-exports landed. **W.5.2/.3 DONE for timer + subscription** (codegen `f2b3363`):
    the generator emits a `PublisherResolver` (keyed on the source entity id) +
    publisher creation + `<C>::init()` State + a move-closure (`||` timer / `|data|`
    subscription) calling `<C as ExecutableComponent>::on_callback` with the source
    callback id; the subscription closure feeds the message CDR into the
    `CallbackCtx` payload. `demo_pkg` has a real publishing timer + a
    message-reading subscription. **Compile + deploy VERIFIED** via the native e2e
    (`fixture_workspace_plans_checks_and_builds` + `deploy_native_self`) with the
    in-repo `play_launch_parser` built + the in-tree SDK env — the generated package
    builds + boots. **Service dispatch also DONE** (codegen `32f56ee`): raw service
    callbacks are non-capturing `extern "C"` fn-ptrs, so the generator emits a
    `SvcCtx{idx}` holding `(State, Resolver)` `Box::leak`'d into a `'static` context
    + a `svc_tramp_{idx}` trampoline that rebuilds a `CallbackCtx::with_reply` over
    the request/response slices, calls `on_callback`, and writes `*resp_len`
    (gated on `uses_std` — `Box`; no_std keeps the noop). `demo_pkg` gains an
    `EchoSrv` whose body reads the request + writes a reply; compile + deploy
    verified by the same native e2e. Inspecting/compiling the emitted Rust caught +
    fixed source-vs-plan-prefixed id bugs + two doubled-brace bugs.

    **Action lifecycle model — characterized + decision half DONE** (`949ca7528`).
    Actions split into two halves with very different constraints:
    - **Decision half (DONE):** goal/cancel callbacks return
      `GoalResponse`/`CancelResponse` enums (not a CDR reply). `CallbackCtx` gains a
      decision sink — `with_goal_decision` / `with_cancel_decision` +
      `set_goal_response` / `set_cancel_response`; the body decides accept/reject,
      the (pending) generated goal/cancel trampoline returns `*out`. Decisions need
      **no executor**, so they wire like the service trampoline (an `ActionCtx`
      `(state, resolver)` + goal/cancel trampolines). Unit-tested
      (`callback_ctx_decision_sink`).
    - **Execution half — the architectural wall:** `ActionServerRawHandle::publish_feedback_raw`
      / `complete_goal_raw` both take **`&mut Executor`**, but callbacks fire
      *inside* `spin_once` with the executor already borrowed → a body **cannot**
      publish feedback or complete a goal from a callback (re-entrancy — unlike
      publishers, which are self-contained). Result/feedback are also *deferred*
      (an action runs over many spins). So the execution half needs a **new
      component execution-tick hook** — a per-spin `Component::tick(&mut TickCtx)`
      that runs *between* callback dispatch with executor access (or a deferred
      action-op queue the executor flushes post-dispatch, like the publish-queue
      idea but for feedback/complete which genuinely need the executor). This is a
      distinct model addition, not a `CallbackCtx` method; **deferred**.
    - **Remaining action codegen:** emit the goal/cancel decision trampolines
      (mirrors the service `SvcCtx`/trampoline, gated on `uses_std`) + accepted =
      noop until the tick hook lands.

    Other W.5.3 remainders: multi-callback shared state (single-callback-owns-state
    model today); no_std service (`static mut` instead of `Box::leak`).)*
  - **W.5.4 — E2E proof (compile→run). DONE** (2026-05-29). `play_launch_parser`
    turned out to be an in-repo Rust crate (`third-party/play_launch_parser`) — built
    it, set `NROS_PLAY_LAUNCH_PARSER`, and with the in-tree SDK env the native e2e
    (`fixture_workspace_plans_checks_and_builds` + `deploy_native_self`) generates →
    compiles → boots the package with real timer + subscription + service executable
    dispatch. `demo_pkg` body publishes / reads / replies.

    **⚠ Codegen relocated (Phase 195.D.5).** The `packages/codegen` submodule was
    retired; `nros` (CLI + codegen) is now the standalone `NEWSLabNTU/nros-cli` repo,
    installed as a prebuilt binary (`scripts/install-nros.sh` / `just setup`). So the
    W.5 split is now: **substrate in this superproject** (`packages/core/nros`:
    `ExecutableComponent` / `CallbackCtx` (+reply +decision sinks) / `PublisherResolver`
    / `declarative_component!`); **codegen in `nros-cli`** (`generate.rs` timer/sub/
    service emission, on its `main` ≈ `32f56ee`). The superproject build uses the
    *installed release* — W.5 codegen is active only once an `nros-cli` release is
    re-cut from a `main` that includes it (the timer/sub/service emission predates
    `nros-v0.3.0`; verify/re-cut before relying on it in a superproject build).

    **Remaining W.5 work items** (substrate → this repo; codegen → `nros-cli`):
  - [x] **W.5.5 — action decision codegen** (`nros-cli` `49f3b7d`). Emits
        `goal_tramp_{idx}`/`cancel_tramp_{idx}` over an `ActionCtx{idx} (state,
        resolver)` `Box::leak`'d context (mirrors the service `SvcCtx`/`svc_tramp`,
        `uses_std && rust_executable_component_path`-gated); goal/cancel rebuild
        `CallbackCtx::with_goal_decision`/`with_cancel_decision` and call
        `on_callback`; accepted = `noop_raw_accepted` until W.5.6. Generated code
        warning-clean (publisher resolver prefixes params when no publishers;
        `state{idx}` is `mut` only for the timer/sub move-closures). Verified:
        `orchestration_generate` (19) +
        `fixture_workspace_builds_generated_service_action_package` e2e (the
        retargeted demo_pkg now declares an `EchoAction` + `cb_act` decision body;
        compiles + links + boots against real nros). Substrate (decision sinks)
        landed earlier in `nros` (`949ca7528`).
  - [x] **W.5.6 — component execution-tick hook** (substrate `nros` `97f306e3c`,
        codegen `nros-cli` `224ca3a`). `ExecutableComponent::tick(&mut State,
        &mut TickCtx)` (default no-op) runs *between* dispatch where the executor is
        free. `TickCtx` exposes the immediate publish path + executor-backed action
        ops (`complete_goal` / `publish_feedback`) via the `ActionExecutor` trait the
        generated runtime impls — `TickCtx` stays `no_std`, free of the
        `rmw-cffi`-gated `Executor`. Test `tick_ctx_publish_and_action_ops`.
        **Codegen:** each std shared instance pushes a tick closure into a
        `thread_local! TICK_ENTRIES`, capturing `Rc::clone`s of the W.5.7 shared
        `Rc<RefCell<State>>` + resolver plus a `[(source entity id, action handle)]`
        array; `GenActionExec` impls `ActionExecutor` over the live executor + that
        array (resolves the action by source entity id); `run_tick_loop` does
        `spin_once(10ms)` then drains every tick entry until halt; the self shim
        (single + multi session) + C-ABI `nros_<sys>_spin` route through it. Gated on
        `has_shared_instance` (std + ≥1 rust executable instance); no_std / native-C
        keep `spin_blocking`/`spin_default`. Verified: `orchestration_generate` (19)
        + e2e pub_sub + service_action compile/link warning-clean + deploy_native_self
        boots/publishes/receives/halts through `run_tick_loop`. A tick-driven action
        *runtime* exchange (client → goal → tick completes) is a follow-up runtime test.
  - [x] **W.5.7 — multi-callback shared state** (`nros-cli` `80342bd`). On a std
        target a component's callbacks share one `State` via a per-instance shared
        prelude: publishers built once into `Resolveri{inst}`, resolver wrapped in
        `Rc`, state in `Rc<RefCell>`; each callback clones the `Rc`s (timer/sub into
        the move-closure, service/action into the leaked ctx) and borrows the shared
        state mutably on dispatch (single-threaded spin ⇒ borrows never overlap).
        Resolver-struct + publisher-builder emission factored into helpers reused by
        the per-callback (no_std) + shared (std) preludes. no_std keeps per-callback
        state — shared no_std needs a `'static` (W.5.8). Verified:
        `orchestration_generate` (19) + both e2e (timer+sub share state;
        service+action share state) compile/link/boot warning-clean.
  - [x] **W.5.8 — no_std service/action codegen** (`nros-cli` `ab33477`).
        `emit_static_service`/`emit_static_action` dispatch real bodies on no_std
        by parking the `(state, resolver)` context in a function-local
        `static mut Option<Ctx>` (no `Box::leak`/`Rc`/alloc); the extern "C"
        trampolines read it via `core::ptr::addr_of_mut!` (sound — single-threaded
        spin). Decision bodies only; no_std action *execution* (feedback/result, the
        tick hook is std-only) is a follow-up. The service/action `!shared` arms emit
        the static path for any rust executable component, keeping the noop only for
        native C/C++. Verified: `generated_no_std_service_action_uses_static_context`
        (emission) + `fixture_workspace_builds_generated_bare_metal_service_action_package`
        compiles the static-ctx service+action to a `thumbv7m-none-eabi` ELF.
- **172.K.5 — per-node multi-domain session routing. DONE** (2026-05-28).
  Executor `NodeBuilder::session_idx` selector (nros-node `ae2b19a19`); generator
  emits a session per distinct `[[domain]]` domain + routes each node via the
  selector (codegen `98392ef`); `nros deploy` stamps node domains from the root
  `[system].[[domain]]` groups (`apply_domain_groups`, codegen `d9d3f89`). The
  `nros check` warning now fires for `[[bridge]]` only (bridge per-node routing
  — topic-forwarding — is the remaining unfinished half, tracked separately).
- **172.E** sandbox hardening; **172.K.7** transport multi-homing — independent.
- **Entity-API tiers → split to [Phase 189](phase-189-entity-api-tiers.md)**
  (cross-cutting client-API refactor, not orchestration — same precedent as
  Phase 187). Collapse the `register_subscription_*_*_*` / `create_*_raw` zoo
  into convenient `create_*` (matching rclcpp/rclrs) over one customizable
  entity **builder** (`fork`/`clone`); design in
  [`docs/design/entity-api-tiers.md`](../design/entity-api-tiers.md). **The
  bridge topic-forwarding runtime half (below) depends on Phase 189 M1** (the
  `.message_info()` + `.session()` knobs).

**Priority.** P2 — none block the MVP workflow; each is an
ergonomic or capability upgrade on top of a working pipeline.

**Depends on.** Phase 126 (archived) — the schema, planner,
checker, generator, and per-board templates this phase extends.

**Subsumes.** Phase 116 (configuration redesign, archived). See *Why
configuration lives here* below.

## Milestones

- [x] **M1 — Planning foundation** (Groups 1–4): config → `nros-plan.json` →
      planner/checker → generated runtime features (L, M, J, K.1–K.6, N · B, C,
      G · A, H, I · D, F).
- [x] **M2 — Config consolidation:** `config.toml` retired into `nros.toml`;
      **Cargo-style manifest fold** — one `nros.toml`, section-discriminated
      (`[workspace]`/`[component]`/`[node]`), walk-up resolution,
      `component_nros.toml` → `[component]` (W.1).
- [x] **M3 — Two-form entry lib** (WP-B): compiled `lib<sys>.a`+header **or**
      source crate+CMake; granular C ABI (`build_executor`/`register_all`/…) +
      runtime `NrosConfig` override (param>env>baked); RMW-set feasibility check;
      multi-domain `SESSION_SPECS`.
- [x] **M4 — Deployment SSOT** (WP-A): one root `nros.toml`; `nros deploy`/`build
      <name>` command-runner; `nros new` scaffolder; `nros doctor` vendor-pin
      check; `metadata --build` auto-collection.
- [x] **M5 — The flip** (WP-C): entry lib generalized to *every* non-bridge
      platform; `render_main`/`EntryKind`-main + dead `SystemConfig` deleted; all
      three "no-compat" replacements executed (generated-`main`, `--launch/
      --system-plan` flags, per-package `system nros.toml`).
- [x] **M6 — `self` proven end-to-end** (QEMU/native): `nros deploy native`
      builds + boots from one root file.
- [x] **M7 — vendor-module proven** (2026-05-28): `nros deploy zephyr-mod` drives
      a real `west` cross-build → boots native_sim (data-plane TAP networking is
      a runtime-env follow-up, not codegen).
- [~] **M8 — vendor-lib proven:** the *pipeline* is proven on host (172.V) —
      `deploy_vendor_lib_real_build_with_stub_lib` drives the real
      emit→link→package against a stub vendor static lib (x86_64, no SDK). Only
      the *actual NVIDIA SPE FSP* link (`NV_SPE_FSP_DIR`) stays blocked on the
      license-gated SDK; unblocks on SDK install or another real vendor lib.
- [x] **M9 — in-binary multi-session routing** (172.K.5): per-node session
      binding. **Multi-domain DONE** (2026-05-28 — `NodeBuilder::session_idx` +
      generator per-domain `SESSION_SPECS` + `[[domain]]`→plan). **Bridge
      topic-forwarding DONE** (2026-05-28): config→plan (`PlanBridge` +
      `apply_bridges`, codegen `64effd0`) + the generator runtime half —
      `register_bridges` emits a bridge node per `connect` endpoint (via the K.5
      `session_idx` selector, idx matched to its `SESSION_SPECS` slot) and, per
      forwarded topic per ordered endpoint pair, the `domain_bridge`-shape relay
      (generic publisher + generic `.message_info()` subscription on the Phase
      189.M1 builder) with `nros-bridge` `bridge_origin` echo suppression.
      `validate_bridges` resolves each topic's type from `interfaces` (errors on
      undeclared / unopened-session / wildcard); the build enables `nros/bridge`.
      The `[[bridge]]` `nros check` warning is dropped (routing now emitted). The
      emitted relay was compile-verified against `nros`; runtime e2e (2 live RMW
      agents) stays gated. See
      [`docs/design/bridge-topic-forwarding.md`](../design/bridge-topic-forwarding.md).

**Phase closes** when M8 lands (or is consciously deferred) + M9; the remaining
independents (172.E sandbox, 172.K.7 multi-homing) can trail. The first-image
toolchain/SDK-distribution work (former W.5) is **split out to Phase 187**
(*Toolchain & SDK distribution*) — see below.

## Background

Phase 126's "Deliberate deferrals" enumerated nine items (the original
A–I) kept out of the MVP to keep the first end-to-end slice tractable;
the configuration work folded in from Phase 116 adds five more (J–N).
The MVP is complete and archived; everything here is a natural next
increment on a working pipeline. Items keep their stable `172.<letter>`
IDs but are now clustered into work groups by area, not by origin.

## Why configuration lives here (subsumes Phase 116)

Phase 116 set out to "redesign configuration" as a standalone concern.
Investigation showed it is not standalone: **configuration is the input
contract of this orchestration pipeline.** The config files are exactly
what the planner consumes; redesigning them in isolation would compete
with — and duplicate — the Phase 126 model that already ships.

The pipeline and its config inputs:

```
  package.xml        identity + msg <depend> + <export>build_type (colcon dispatch)
  component nros.toml reusable: linkage, metadata, default ns/params/remaps
  system nros.toml    deployment: target{triple,board,rmw,network,transport},
                        components, overlays(per-instance), scheduling(RT), build
  launch files (opt)  node graph / topology
        │
        ├─ MODE 1 DIRECT   one node, hand-written main(), reads its nros.toml
        │                  subset via Config::from_toml (include_str! on embedded).
        │                  Replaces config.toml. Keeps copy-out-template examples.
        └─ MODE 2 PLANNED  nros plan → nros-plan.json → nros build → generated
                           main() → ONE binary, all nodes wired at compile time.
```

**One schema (the Phase 126 component/system `nros.toml`), two modes.**
A trivial single-node app reads a subset directly (no launch, no
planner, no generated `main`); multi-node systems go through the
planner. `package.xml` owns identity + msg deps in both modes;
`nros.toml` owns all nano-ros config.

What 116 wanted, mapped to this model:

| 116 concern | State in the 126 model |
|---|---|
| RMW selection | already `system.target.rmw`; wire it everywhere (172.M) |
| per-node options | already `system.overlays` + `component.overrides` — done |
| RT / scheduling | already `SchedContextConfig`; multi-tier is 172.G |
| peripheral/network | **schema gap** — add `target.network`/`transport` (172.J) |
| `config.toml` sprawl | retire into direct-mode `nros.toml` (172.K) |
| `nros.toml` name clash | bridge (Phase 124 `run_from_config`) vs orchestration — rename bridge (172.L) |

The single-`[node]` schema and the package.xml-vs-`nros.toml` (A/B)
framing explored in the archived 116 doc are **superseded** by this
component/system model.

> **Superseded by the revised deployment model (2026-05-28).** The
> `MODE 2 PLANNED → ephemeral generated main() under build/` pipeline
> above, and the per-package "system `nros.toml` with
> `target.{triple,board}`", are revised by *Revised deployment model*
> below: the generated unit is a **library** (not a `main`), all config
> lives in **one root `nros.toml`** (not per-package), and platform
> deployment is a **command-runner** (`nros deploy`). The plan IR
> (`nros-plan.json`) and Groups 1–4 carry over unchanged.

## Revised deployment model (2026-05-28)

Groups 1–4 complete the **planning** half (config → `nros-plan.json` →
generated wiring). A design review (2026-05-28) revised the
**deployment** half: how that wiring becomes a shippable artifact across
native, vendor-library, and vendor-owned-build targets. Group 5 below
implements it.

### Build ownership — the axis that drives everything

Real targets split by **who owns the final build** — and all three
already have in-tree precedent:

| Model | Final build driven by | nano-ros is | Precedent |
|---|---|---|---|
| **self** | cargo / cmake (nano-ros) | the whole binary | native, bare-metal QEMU |
| **vendor-lib** | cargo / cmake (nano-ros), linking a vendor static lib | the app | Orin SPE (`libtegra_aon_fsp.a` via `NV_SPE_FSP_DIR`) |
| **vendor-module** | the vendor's `make` / `west` / `idf.py` | a guest module | PX4 `EXTERNAL_MODULES_LOCATION`, NuttX external app, ESP-IDF component, Zephyr module; QNX `mkifs` packaging |

Ownership decides implicit-vs-explicit: **self / vendor-lib can be
implicit** (`nros build <name>`); **vendor-module is eject-only** (the
vendor drives — you cannot `nros build` a PX4 firmware).

### The entry lib ships in one of two generally-accepted forms

The generated wiring is a **library with a granular C ABI**
(`nros_<sys>_register_all(exec)`, per-node `register_<node>`,
`build_executor()`, `Config`) — the universal import unit every model
consumes. It ships as:

- **compiled** — `lib<sys>.a` + cbindgen `<sys>.h` (nano-ros owns the
  toolchain → must match `[deploy].target`).
- **source** — generated crate + a vendor-includable CMake fragment
  (`add_subdirectory` + corrosion); the **vendor compiles it in its own
  toolchain**, so toolchain coherence is free.

Vendor-owns-toolchain (PX4, Zephyr) → source form; nano-ros-owns-toolchain
(native, Orin link) → compiled form. This generalizes today's Zephyr
`rust_cargo_application` staticlib + Phase 175 corrosion path, and
collapses the per-platform `EntryKind` / `render_main` branching —
platform **startup** moves out to the deploy side; the generated wiring
is one neutral lib.

### Single root `nros.toml` — the SSOT

One config file at the workspace root holds everything; `deploy/<name>/`
code dirs are *referenced by path*, not authored as separate packages:

```
ws/  nros.toml          # SSOT: [workspace] [system|systems.*] [deploy.*] [overlays.*] [[bridge]]
     src/<component>/    # component code (+ optional intrinsic nros.toml, reusable)
     launch/*.xml        # topology (referenced from root)
     deploy/<name>/      # ejected: hand startup / vendor module shell / linker script ({self})
     build/              # generated wiring lib (ephemeral, gitignored)
```

```toml
[workspace]
default = "native"

[system]                                   # the old "entry package", now just root config
launch     = "launch/sys.launch.xml"
components = ["talker_node", "listener_node"]
rmw        = "zenoh"     # SSOT default RMW
domain_id  = 0           # SSOT default domain

[deploy.native]                            # self — main generated, no code dir
target = "x86_64-unknown-linux-gnu"

[deploy.mcu]                               # vendor-module (Zephyr) — source form
kind = "vendor-module"; target = "zephyr"; board = "nucleo_h753zi"; rmw = "xrce"
self = "deploy/mcu"
build = ["west build -b {board} -d build/mcu {self}"]

[deploy.orin]                              # vendor-lib (Orin SPE) — compiled form
kind = "vendor-lib"; target = "armv7r-none-eabihf"; self = "deploy/orin"
vendor.dir = { env = "NV_SPE_FSP_DIR" }; vendor.pin = "spe-fsp 36.3"
build   = ["arm-none-eabi-gcc {self}/startup.o {entry_lib} -L{vendor.dir}/lib -ltegra_aon_fsp -T {self}/spe.ld -o build/orin/spe.elf"]
package = ["python3 {vendor.dir}/tools/spe_sign.py build/orin/spe.elf -o build/orin/spe.bin"]

[deploy.drone]                             # vendor-module (PX4)
kind = "vendor-module"; target = "px4"; board = "px4_fmu-v6x_default"; rmw = "uorb"
self = "deploy/drone"
vendor.dir = { env = "PX4_AUTOPILOT_DIR" }; vendor.pin = "PX4 v1.15.0"
build = ["make -C {vendor.dir} {board} EXTERNAL_MODULES_LOCATION={self}"]
```

- **`[system].rmw` + `[system].domain_id` = the SSOT** for RMW + domain.
  Component `nros.toml` never sets them (deployment decides).
  `[deploy.<name>]` may override per target; host env
  (`ROS_DOMAIN_ID` / `NROS_RMW`) overrides at runtime. Precedence:
  **deploy > `[system]` > default**, host **env > baked**.
- `target.{triple,board}` are **per-deploy** (one system → many
  platforms), not in `[system]`.
- Multiple deploy targets = multiple `[deploy.<name>]` tables. Multiple
  systems = `[systems.<name>]` + per-deploy `system = "<name>"`.

### `nros deploy` — command-runner

`nros deploy <name>` = assert `vendor.pin` → emit the form (compile
`.a`, or generate source into `{self}`) → run `[deploy.<name>].build[]`
→ run `package[]`, substituting `{self}` (= `deploy/<name>/`),
`{entry_lib}`, `{entry_src}`, `{entry_header}`, `{board}`, `{target}`,
`{vendor.dir}`. **No per-vendor code in nano-ros** — vendor knowledge
lives in the user's `build[]` / `package[]` shell lines (the real
deployment is the vendor workspace's concern). nano-ros contributes the
plan IR, the entry lib, form emission, sequencing, var/config injection,
and pin assertion. Per-vendor *adapters* stay out until a vendor earns one.

### Config lowering (runtime-config path)

Generation lowers the plan's config per **(net-owner × host/embedded)** —
no new policy, formalizing the compile-time-domain rule + Phase 173.7:

- **host** → bake nothing; entry `resolve_config()` reads env at runtime.
- **embedded + NanoRosOwned** → bake board `Config` consts + a shared
  `<sys>_config.h`.
- **embedded + RtosOwned** → domain/locator baked (Kconfig /
  `app_config.h`); NIC config → a vendor Kconfig/defconfig fragment in
  `{self}`, deploy merges via the vendor's own include hook
  (`EXTRA_CONF_FILE`, defconfig merge) — never editing the pinned tree.
- **vendor-link (IVC)** → channel id in the shared header (entry lib +
  hand startup both include).
- **vendor pub/sub (uORB)** → no net config; node params baked.

The C-ABI entry takes an optional `Config` override (`cfg = NULL` ⇒
baked/env). Precedence **param > env > baked**.

### Bridges

**A node belongs to one session (one rmw + domain); a bridge spans
sessions and is not a node.**

- **OUT bridge (default)** — a separate deployable, runtime-configured
  by `nros-bridge.toml` (the 172.L `[[node]]` / `[[bridge]]` file); the
  app system is unaware. Host / gateway-with-OS, lifecycle decoupled.
- **IN bridge** — build-time `[[bridge]]` in root `nros.toml` →
  generated `Executor::open_multi([SessionSpec])` + per-node session
  assignment (`[[domain]]` groups); one firmware. For single-binary
  gateways (no process spawning). Same-RMW cross-transport/domain is the
  common embedded case; cross-RMW in-binary makes `build.rmw` a *set* and
  is host/gateway-Linux-mostly — `nros check` warns when a target can't
  link the required RMW set.

### Eject gradient — no long commands at any stage

Config always lives in `nros.toml`; ejecting materializes *code*, never
re-types config:

- **implicit** — root `[deploy.<name>]` profiles → `nros build` /
  `nros build <name>` (native / sim). No flags.
- **eject deploy** — `nros new --deploy <name> --kind <k> --target <t>`
  scaffolds `deploy/<name>/` (vendor shell / hand startup) + appends
  `[deploy.<name>]` to root → `nros deploy <name>`. Required for
  vendor-module; optional otherwise.

`nros build <name>` resolves `<name>` as a `[deploy.<name>]` profile or
a `deploy/<name>/` dir; bare `nros build` uses `[workspace].default`.

## Work groups (parallelization)

Four groups, each owning a largely disjoint area of the tree, so they
can be staffed and shipped in parallel:

| Group | Area | Items | Intra-group order |
|-------|------|-------|-------------------|
| **1 — Configuration & build inputs** | config files, `SystemConfig` schema, examples, colcon/`nros build` RMW wiring, `.cargo/config.toml` | L, M, J, K, N | L, M (small unblockers) → J (schema) → K (migration) → N (audit/docs) |
| **2 — Planner & scheduling** | host planner dataflow, plan-schema sched representation, generated executor wiring | B, C, G | B → C → G |
| **3 — Generated-runtime capabilities** | `nros-orchestration` runtime, generated `main`, plan representation of runtime features | A, H, I | independent (A largest) |
| **4 — Host tooling & DX** | host CLI only; no runtime/plan-schema coupling | D, E, F | independent |
| **5 — Revised deployment model (ACTIVE, no-compat)** | root `nros.toml` SSOT, two-form entry lib, `nros deploy` command-runner, `deploy/` dirs, config lowering, bridges, eject gradient, per-vendor templates, migration + deletion | 3 packages: **WP-A** config & CLI, **WP-B** generator, **WP-C** platforms & cutover | **WP-A ‖ WP-B** (parallel; agree the interface at kickoff) → **WP-C** (sequential, after both). See *Work packages*. |

**Shared contract.** Groups 1–3 all touch the `nros-plan.json` schema
(Group 1 feeds `SystemConfig` → plan inputs; Group 2's planner writes
the plan; Group 3's runtime reads it). Schema changes must be
**additive + version-bumped** and coordinated across these three.
**Group 4 is fully independent** — it only reads existing artifacts.

> **Schema log (`PLAN_VERSION`).** v1 → **v2** (Group 2, 172.B/C):
> two additive top-level arrays on `NrosPlan`, both
> `#[serde(default, skip_serializing_if = "Vec::is_empty")]` so a
> plan with neither serializes byte-identically to v1:
> - `callback_chains: Vec<PlanCallbackChain>` (172.B) — `{ id,
>   callbacks, links: [{from,to,topic}], inferred }`.
> - `callback_groups: Vec<PlanCallbackGroup>` (172.C) — `{ id,
>   kind: CallbackGroupKind (mutually_exclusive|reentrant), callbacks,
>   inferred }`.
>
> No `PLAN_VERSION` bump for 172.G — it adds no field; `sched_contexts`
> already existed. But the planner now **consumes**
> nros.toml `[[scheduling.contexts]]` (172.G), previously parsed-and-
> ignored. **Group 1 (config owner):** that key is now live — a
> declared tier id is matched against each callback's `group`.
>
> 172.A adds one additive top-level field on `NrosPlan` (still v2):
> `lifecycle: Option<PlanLifecycle>` (`{ autostart:
> none|configure|active }`), `#[serde(default,
> skip_serializing_if = "Option::is_none")]`, read from nros.toml
> `[lifecycle]`. **Group 1 (config owner):** `[lifecycle]` is now a
> live key.
>
> Group 1/3 agents: rebase onto these. Two **pre-existing** schema
> bugs found + fixed in the same pass (HEAD's `orchestration_schema`
> round-trip was already red): the `PlanEntity` `callback: Option`
> fields and `PlanBuildOptions.transports: Vec` both serialized
> `null`/`[]` while the golden fixtures omit them — added
> `skip_serializing_if` to all five.

## Work items

> **Groups 1–4 are the completed planning foundation** (config →
> `nros-plan.json` → planner → runtime features). Group 5 reuses all of
> it unchanged *except* the parts the revised model replaces (the
> generated-`main` emitter, the flag-driven `nros build`, the
> per-package `system nros.toml`). The item records below are kept for
> history; **the active work is Group 5.**

### Group 1 — Configuration & build inputs

**Parallel lane.** Touches the config inputs (`package.xml` stays
identity-only; `nros.toml` schema; examples; colcon task; `nros build`;
`.cargo/config.toml`) and the direct-mode `Config::from_toml` path. Do
L + M first (small, unblock the rest), then the schema (J), then the
example migration (K), then the audit/docs (N).

- [x] **172.L — Resolve the `nros.toml` name collision.** DONE 2026-05-27 —
      bridge config renamed to `nros-bridge.toml` (`run_from_config` is
      path-agnostic; updated doc comments + the book page + SUMMARY link).
      Two
      incompatible schemas currently share the filename: the Phase 124
      **bridge** config (`nros_bridge::run_from_config`, runtime
      `[[node]]`/`[[bridge]]` multi-RMW forwarding) and the Phase 126
      **orchestration** config (build-time component/system). Different
      lifecycles — they cannot share a schema. Orchestration keeps
      `nros.toml`; rename the bridge config to `nros-bridge.toml`
      (update `run_from_config` default, docs `book/src/reference/nros-toml.md`,
      and any callers). No example ships the bridge file today, so the
      blast radius is small.

- [x] **172.M — Wire RMW from `system.target.rmw`.** DONE 2026-05-27 — the
      orchestration generator already threads `build.rmw`; the colcon task's
      hardcoded `zenoh` + dead `find_package(NanoRos)` were fixed (RMW from
      `NANO_ROS_RMW` env via `resolve_rmw()`; platform from the parsed token;
      zephyr `prj-<rmw>.conf` overlay).
      Make
      `system.target.rmw` the single source for RMW selection across
      every build path: the colcon task (`colcon_nano_ros/task/nros/build.py`)
      currently **hardcodes `-DNANO_ROS_RMW=zenoh`** and references the
      dead `find_package(NanoRos)` (removed in Phase 140) — both must be
      fixed; `nros build` threads the Cargo feature / CMake `-D` / Zephyr
      `prj-<rmw>.conf` from `target.rmw`; direct-mode `Config::from_toml`
      reads it. Manual `cargo`/`cmake` builds keep working by passing the
      selection by hand.

- [x] **172.J — Peripheral/network config in `SystemConfig`.** DONE 2026-05-27 —
      Phase 173.5 already parses `[[transport]]` (kind/ip/device/baudrate/rmw/
      locator); added the remaining `config.toml [network]` fields `mac` +
      `gateway` to `PlanTransport` (+ `BoardTransportConfig::{set_mac,set_gateway}`
      + generator emission + validate). (172.K.4/K.7 added wifi `ssid`/`password`
      + the `interfaces` multi-homing list on top.)
      Original scope: extend
      the Phase 126.A `SystemConfig` schema with `target.network`
      (ip/mac/gateway/prefix) and `target.transport`
      (ethernet/wifi/serial + their params). Today this lives only in
      the per-example `config.toml` and is invisible to the planner.
      Scope: schema fields + `nros check` validation + consumption in
      126.D generated `main` (bake into the generated binary) and in
      direct-mode `Config::from_toml`. This is the one genuine schema
      gap from 116.

- **172.K — Retire `config.toml` into `nros.toml` (direct mode).** Define
      the single-node **direct mode**: a hand-written one-node app reads its
      `nros.toml` via `Config::from_toml` (compile-baked with `include_str!`
      on embedded, fs/env on hosted) — no launch file, no planner, no
      generated `main`; the copy-out-template examples keep their hand-written
      `main()`. Schema = `[node]` (domain/namespace) + top-level
      `[[transport]]` (id-addressable session: kind/ip-CIDR/mac/gateway/rmw/
      locator/device/baudrate/ssid/password/interface) + `[node.rt]`
      (scheduling). Nodes bind to transports by `id` (0/1 implicit, N explicit).
      **Approved design: [`docs/design/configuration-and-transports.md`](../design/configuration-and-transports.md).**
      Migrate the 88 example `config.toml`, 86 `include_str!("config.toml")`
      sites, the 8 board `Config::from_toml` parsers, and the 5 board
      `build.rs`; then delete `config.toml`. Staged sub-items:

  - [x] **172.K.1 — direct-mode parser support (additive) + pilot.** Board
        `Config::from_toml` parses the new `[[transport]]`/`[node]`/`[node.rt]`
        shape **alongside** the legacy `[network]`/`[zenoh]`/`[scheduling]`
        (section parser handles `[[...]]` array-of-tables + dotted sections),
        so boards + examples migrate independently with no flag day. Pilot:
        `nros-board-mps2-an385` + the qemu-arm-baremetal rust talker → `nros.toml`,
        `cargo check` (thumbv7m) green. (`38d342a89`.)
  - [x] **172.K.2 — roll out the 7 remaining board `from_toml` parsers.**
        Done (`96120466d`): freertos (+`[node.rt]` scheduling, CIDR→netmask),
        threadx-linux (+`interface`), threadx-riscv64 (CIDR→netmask), esp32
        (+wifi via `IpMode`), esp32-qemu, stm32f4 (+`usart_index`),
        nuttx-qemu-arm (no MAC) — all additive alongside the legacy arms.
        freertos + threadx-linux compile-verified via their examples; the
        prefix/serial boards mirror the verified mps2 pilot (compile-checked in
        K.3 per-platform builds). The 5 board `build.rs` bakers move with their
        examples in K.3.
  - [x] **172.K.3 — migrate the 88 example `config.toml` → `nros.toml`.** DONE
        2026-05-27. **Rust** (40) — `include_str!` switched; board `from_toml`
        parses the shape. **C/C++** (47, freertos/nuttx/threadx-{linux,riscv64}
        × c+cpp) consume config via the CMake `nano_ros_read_config` →
        `NROS_APP_CONFIG` path, so **both** parser copies
        (`cmake/NanoRosConfig.cmake` + `packages/core/nros-c/cmake/NanoRosReadConfig.cmake`)
        were taught the `[node]`/`[[transport]]`/`[node.rt]` shape (additive) and
        each CMakeLists `nano_ros_read_config` path repointed. 0 source
        `config.toml` remain. Verified: representative Rust cargo-checks (mps2/
        freertos/threadx-linux) + both CMake parsers emit correct
        `NROS_APP_CONFIG` from a converted file. Full per-platform cross-build
        rides the K.6 `build-all`. (Board `build.rs` needed no change — they read
        `.cargo/config.toml` only.)
  - [x] **172.K.4 — planned-mode parity (submodule `colcon-nano-ros`).** DONE
        2026-05-27 (`ea695e3` on colcon-nano-ros main; superproject pointer
        bumped). `PlanTransport` gained `id` + wifi `ssid`/`password`;
        `TransportKind::Wifi` (+`cargo_feature "wifi"`); `validate_transports`
        Wifi kind + ssid/password=wifi-only; generator emits
        `c.set_ssid`/`c.set_password` in `apply_transport_config` (matching the
        new no-op-default `BoardTransportConfig::{set_ssid,set_password}`
        superproject setters); `SystemComponent` gained `transport: Option<String>`
        carrying the per-instance bind through system config → plan. Additive +
        serde-default (existing plans round-trip). 47 lib + all integration tests
        green. The full `SESSION_SPECS`-by-id wiring is the K.5 runtime step;
        K.4 lands the schema + generator + the binding field.
  - [x] **172.K.5 — per-node multi-domain session routing. DONE 2026-05-28.**
        Landed across three commits: the executor selector
        `NodeBuilder::session_idx` (nros-node `ae2b19a19`); the generator emitting
        a `SESSION_SPECS` entry per distinct `[[domain]]` domain + routing each
        node to its slot via the selector + `open_multi` (codegen `98392ef`); and
        `nros deploy`'s `apply_domain_groups` stamping node domains from the root
        `[system].[[domain]]` groups (codegen `d9d3f89`). The `nros check`
        `pending_routing_warning` now covers `[[bridge]]` only. **Bridge per-node
        routing (topic-forwarding gateway) remains** the unfinished half — node
        *placement* (multi-domain) is done; bridge *forwarding* is separate.
        Original scoping notes (now historical):
        Bind a node to a session by transport `id` (not just `rmw`); only
        required for **case D** (segregated same-rmw sessions) in the taxonomy.
        **SUBSUMED by WP-B / bridges** (2026-05-28): per-node session
        assignment for in-binary multi-domain/bridge builds is exactly this
        binding — implement it there against the `[[domain]]`/`[[bridge]]`
        root config.
        **Scope re-assessed 2026-05-28 (TRACTABLE — executor work is small;
        bulk is codegen + planner):** an executor review found most multi-session
        machinery already exists — `open_multi` opens a session **per spec with
        no rmw-dedup** (so same-rmw / different-domain sessions can coexist),
        `session_at_mut(idx)` already indexes (`0`=primary, `N`=`extra_sessions
        [N-1]`), and **entity routing by node session already works via the
        `_on` register variants** (`register_*_on(node_handle, …)` →
        `nodes[node].session_idx` → `session_at_mut`; the generator already
        emits `_on`). The only **executor gap** is a node→session **selector**:
        `create_node_on(name, rmw)` resolves via `resolve_session_slot`, which
        keys on **rmw + locator only** (domain-blind) and matches by a *prior
        NodeRecord* (so it won't bind to `open_multi`'s pre-opened sessions —
        opens a duplicate). Fix = one small method, `NodeBuilder::session_idx(u8)`
        / `Executor::create_node_on_session(name, idx)`, that sets
        `NodeRecord.session_idx` directly and bypasses `resolve_session_slot`
        (the generator supplies the index, so domain-aware *resolution* isn't
        needed). Work: **(1)** `nros-node` — the small `session_idx` selector
        — **DONE 2026-05-28 (`ae2b19a19`):** `NodeBuilder::session_idx(u8)` binds
        a Node to a pre-opened slot (validated), bypassing `resolve_session_slot`;
        unit-tested (slot 0/1 bind + out-of-range reject). **Remaining
        (codegen/planner bulk):** **(2)** generator — emit `SESSION_SPECS` from
        the distinct `[[domain]]` domains (not just multi-transport), a
        node→session index in `NODES` (today `render_nodes` hardcodes
        `domain_id: None`), `build_component_node` routing through the selector,
        and `build_executor` → `open_multi` when multi-domain; **(3)** plan —
        `PlanNode` carries `session_idx`/domain; **(4)** planner/deploy —
        `[[domain]]` (root `[system].domain`) → plan; **(5)** drop the domain
        half of the `nros check`
        `pending_routing_warning`. The warning stays as the guard until this
        lands. *(`[[bridge]]` is a topic-forwarding gateway, not node placement
        — out of scope for K.5; that's the W.5/bridge-data-plane line.)*
  - [ ] **172.K.7 — multi-homing `[[transport]].interfaces` (list).** A single
        session spanning several NICs as one merged graph (taxonomy cases B/C —
        the common "node reachable on multiple interfaces" need, what stock
        DDS/zenoh do natively). Generalize the current single `interface` field
        to a list; generator maps it per backend (zenoh listen/connect per NIC +
        scouting iface; Cyclone `<Interfaces>`; Fast DDS whitelist). Distinct
        from K.5 (merge vs segregate). Design:
        [`docs/design/configuration-and-transports.md`](../design/configuration-and-transports.md)
        ("Two axes" taxonomy).
        - [x] **Schema + plumbing landed** (2026-05-29). `PlanTransport.interfaces:
              Vec<String>` (serde default, skip-when-empty) + `validate_transports`
              rejects it on serial/can (ethernet/wifi only); the generator emits a
              `c.set_interfaces(&[…])` board-Config call (mirrors `set_ssid`/`set_mac`),
              backed by a default-no-op `BoardTransportConfig::set_interfaces` seam;
              both CMake parsers (`NanoRosConfig.cmake`, nros-c `NanoRosReadConfig.cmake`)
              accept the TOML array `interfaces = ["eth0","eth1"]` (legacy scalar
              `interface` mirrored in) → new `NROS_CONFIG_INTERFACES` list var. Tests:
              `transport_tests::{multi_homed_interfaces_parse_and_validate,
              interfaces_absent_round_trips_empty_and_skips_serialization,
              interfaces_are_ethernet_wifi_only}` +
              `multi_homed_interfaces_emit_set_interfaces_call`.
        - [ ] **Per-backend *wire* emission (the merge) — deferred.** The `interfaces`
              list plumbs cleanly to a no-op `set_interfaces` seam but still changes no
              backend's actual NIC binding. Three blockers, in order:
              1. **Multi-endpoint `SessionSpec` (runtime, `nros`).** `SessionSpec::new(rmw,
                 locator)` carries **one** locator; the generator emits one spec per
                 `[[transport]]`. Multi-homing = one session listening/connecting on N
                 NICs as one graph → `SessionSpec` must hold a list of endpoints and
                 `open_multi` wire them. This is an `nros` runtime change, not codegen —
                 the prerequisite for any real merge.
              2. **Backend mapping (generator + RMW layer).**
                 - *zenoh* — one `listen`/`connect` endpoint per NIC +
                   `scouting.multicast.interface`. But nano-ros nodes are zenoh-**pico
                   clients** with a single locator to the router, so node-level
                   multi-listen is largely the router's concern → needs a decision on
                   what (if anything) `interfaces` means for a pico client before any
                   emission.
                 - *Cyclone* — emit `<General><Interfaces>`. The real, meaningful case,
                   but the generator emits **no** Cyclone config today (it lives in
                   `session.cpp`'s `kEmbeddedCycloneConfig` / `CYCLONEDDS_URI` env) → needs
                   a generator → Cyclone-config emission path.
              3. **No multi-NIC target to verify.** Every board today has **one** NIC, so
                 the merge can't be exercised end-to-end. A hosted multi-NIC Cyclone build
                 is the first place this is both meaningful *and* testable — making Cyclone
                 the only backend where finishing K.7 is worthwhile right now.
  - [x] **172.K.6 — drop the legacy arms + delete `config.toml`.** DONE
        2026-05-27. All 88 examples + 2 nros-bench fixtures on `nros.toml`
        (0 source `config.toml` repo-wide); legacy `[network]`/`[zenoh]`/
        `[scheduling]`/`[platform]`/`[wifi]`/`[serial]` arms removed from all 8
        board `from_toml` parsers + both CMake parsers (`NanoRosConfig.cmake`,
        `nros-c/NanoRosReadConfig.cmake`) — parsers accept only the direct-mode
        `[node]`/`[[transport]]`/`[node.rt]` shape. Last runtime consumers
        migrated first (3 logging-smoke bins; `nros new` scaffolder → `nros.toml`,
        colcon-nano-ros `d37a692`). **Verified: `build-all` green across every
        platform** (board drops + the CMake C/C++ path); both CMake parsers
        parser-driven. Also fixed along the way: zephyr cyclonedds graph-types
        build (177.36, landed on main `4c6ce2520`) + the converter `#`-in-serial-
        locator bug + the rust-example CMakeLists `nano_ros_read_config` repoint.

- [x] **172.N — Audit `.cargo/config.toml` to dep-injection only.** DONE
      2026-05-27. **Audit PASS:** every example `.cargo/config.toml` holds only
      legit cargo sections (`[patch.crates-io]` dep-injection + `[build]`/
      `[target]`/`[env]`/`[unstable]` cargo knobs) — zero nano-ros semantic
      config (locator/domain/ip/mac) leaked (the one grep hit was a comment).
      Rewrote `book/src/user-guide/configuration.md` around the one-lane-per-file
      model (file-ownership table + the `nros.toml` `[node]`/`[[transport]]`/
      `[node.rt]` shape + direct-vs-planned read modes + link-vs-active RMW).
      **Follow-up (separate doc sweep, not 172):** ~10 getting-started/reference
      book pages still show the retired per-example `config.toml` in tutorials
      (`first-node-rust`, `freertos`, `bare-metal`, `cli`, …) — update to
      `nros.toml` for new-user correctness.

### Group 2 — Planner & scheduling

**Parallel lane.** Host-side planner dataflow analysis + the
`nros-plan.json` scheduling representation + generated executor wiring.
B infers the chains, C groups callbacks from those chains, G consumes
the grouping into multi-tier scheduling — so run B → C → G.

- [x] **172.B — Automatic callback-chain inference.** Infer
      callback execution chains (which callback feeds which) from
      the topic graph instead of requiring explicit bindings.
      Scope: dataflow analysis in the planner; emit inferred chains
      into the plan with an override escape hatch.
      *Done:* `infer_callback_chains` (planner.rs) walks
      instance publisher→subscriber dataflow, union-finds
      weakly-connected components, Kahn-topo-orders each into a
      `PlanCallbackChain`; emitted into the plan (`callback_chains`).
      `inferred: true`; an explicit `[[chain]]` override sets it
      false. 3 unit tests.

- [x] **172.C — Automatic callback-group inference.** Derive
      callback groups (mutually-exclusive vs reentrant) from the
      graph + scheduling annotations rather than hand-authored
      groups. Scope: planner heuristic + `nros-plan.json`
      representation + generated `SchedContext` binding.
      *Done:* `infer_callback_groups` derives groups from the
      172.B chains — each chain → one `mutually_exclusive` group
      (dataflow-coupled stages serialize); each chain-less callback
      → its own `reentrant` singleton group (no coupling ⇒
      concurrent-safe). `PlanCallbackGroup` + `CallbackGroupKind`
      in the plan; 3 unit tests. The generated single-threaded
      executor already serializes all callbacks, so group **kinds**
      become observable only with the 172.G multi-tier executor —
      the runtime enforcement of `reentrant` concurrency lands there.

- [x] **172.G — Multi-tier scheduling.** Extend the single-tier
      `SchedContext` model to multiple scheduling tiers (e.g. a
      high-rate RT tier + a best-effort tier within one executor).
      Depends on the Phase 110 scheduling primitives. Scope: plan
      schema for tiers + generated multi-tier executor wiring.
      *Done (config-driven):* the runtime already dispatches across
      Phase 110.C's three `Priority` buckets (FIFO/EDF by class) and
      the generated `run_executor` already creates **N** sched-contexts
      in one executor + binds callbacks — multi-tier was wired at the
      runtime + codegen layers. The gap was the **planner**, which
      hardcoded a single `best_effort` `default_executor` and bound
      every callback to it. Now `collect_sched_contexts` reads the
      nros.toml `[[scheduling.contexts]]` tiers (author-declared, not
      inferred — launch files carry no scheduling, source metadata only
      a `group`) into the plan's `sched_contexts`; each callback binds
      to the tier whose id equals its `group` (**group name = tier id**),
      falling back to `default_executor` (still emitted only when used,
      so single-tier plans stay byte-identical). The binding onto a
      declared tier carries its priority + `source: "nros.toml"`. 4
      tests (3 unit + 1 end-to-end `plan`→`check` in
      `orchestration_cli.rs`). **Tier→callback binding is by `group`
      name only**; an explicit `[[scheduling.bindings]]` table
      (decoupling group names from tier ids) is a deferred follow-up if
      that proves too rigid.

> **172.G binding source.** nros.toml `[scheduling]`
> (`config::SchedulingConfig`) was a fully-designed but **unwired**
> schema — parsed as raw `Value`, only the `[build]` block consumed.
> 172.G wires `[[scheduling.contexts]]` through `schema_plan_json`.
> `config::SchedContextConfig` already mirrors `PlanSchedContext`
> field-for-field, so a TOML context maps straight onto a plan tier
> (absent optional keys normalised to null/defaults).

### Group 3 — Generated-runtime capabilities

**Parallel lane.** Extends the `nros-orchestration` runtime + the
generated `main` + the plan's representation of runtime features. The
three are independent of each other; A (lifecycle) is the largest.

- [x] **172.A — Lifecycle node orchestration.** Model
      managed-lifecycle nodes (configure / activate / deactivate /
      cleanup transitions) in the plan schema + generated runtime.
      Today every instance is a plain node brought up once at boot.
      Scope: lifecycle state in `nros-plan.json`, transition
      callbacks in the generated runtime, `nros check` validation
      of lifecycle graphs.
      *Done (system-level, config-driven):* the REP-2002 state
      machine (`nros-core`/`nros-node` `lifecycle*.rs`) + the
      executor services (`Executor::register_lifecycle_services`,
      `lifecycle-services` feature) already exist. The plan now
      carries an optional `lifecycle: { autostart: none|configure|
      active }` block (`PlanLifecycle` / `LifecycleAutostart`),
      read from nros.toml `[lifecycle]` (`collect_lifecycle`).
      Codegen emits `apply_lifecycle(&mut executor)` — a no-op for
      unmanaged plans (no feature, byte-equivalent), else
      `register_lifecycle_services()` + the boot autostart
      transitions; `run_executor` calls it after binding callbacks,
      and a managed plan enables `nros/lifecycle-services`. `nros
      check` validates via the `NrosPlan` parse (autostart enum). 4
      tests (planner unit + plan→check e2e + managed/unmanaged
      codegen); the no-op path is compile-checked by the real-build
      e2e suite. **Scope note:** the runtime models **one** lifecycle
      SM per executor, so this is *system-level* (the generated
      binary's node is managed). **Deferred (needs new runtime
      core):** per-instance lifecycle (multiple managed nodes in one
      binary, requiring a per-node SM registry), component-provided
      transition callbacks (today's transitions take the
      default-success path), and gating callback dispatch on the
      `Active` state.

- [x] **172.H — Runtime parameter-override persistence.** Persist
      runtime parameter overrides (set after boot) across restarts.
      Today parameters come from the plan + launch manifest at
      generation time only. Scope: a persistence backend (flash /
      file) + load-on-boot in the generated runtime. **Landed
      (hosted file backend):** a `ParamStore` trait + `NullParamStore`
      (no-op) + `FileParamStore` (`std`, atomic text file) in
      `nros-params`, with the `ParameterServer` tracking a `dirty` flag
      on `set`/`unset`. `Executor::enable_parameter_persistence[_with]`
      (`nros-node`, `param-services`) overlays persisted overrides onto
      the declared defaults at boot, and the spin loop flushes the full
      set whenever a runtime `set_parameters` changed a value
      (`flush_param_store` gated on `take_dirty`). The plan carries an
      optional `param_persistence: { backend, path }`
      (`PlanParamPersistence`, read from nros.toml `[param_persistence]`
      via `collect_param_persistence`); codegen emits
      `apply_param_persistence` — a no-op for plans without the block
      (no param services, byte-equivalent), else register-services +
      declare-params + attach `FileParamStore` — called from
      `run_executor` after `apply_lifecycle`, and a persistence plan
      pulls `nros/param-services`. `nros` re-exports
      `FileParamStore`/`ParamStore`. Tests: store round-trip + dirty
      tracking + boot-overlay→flush (nros-params), planner collect,
      generator render (no-op + file), and a real
      generate→build→link of a persistence package in
      `orchestration_e2e`. **Deferred:** flash / NVS backends for
      embedded targets (the trait is backend-agnostic; only the hosted
      file backend ships today), and array-typed parameter values
      (scalars only persist in v1).

- [x] **172.I — Generated shared state.** Support shared state
      between components in one generated binary (e.g. a shared
      blackboard / typed shared region) instead of every component
      owning isolated state. Scope: plan representation + generated
      `static` shared-region tables + access discipline. **Landed** —
      `nros.toml` `[[shared_state]]` entries (`id` + `bytes`) flow
      `collect_shared_state` (planner) → `NrosPlan.shared_state:
      Vec<PlanSharedRegion>` (additive, skip-if-empty so v1 plans stay
      byte-identical) → `render_shared_state` emits `pub static
      SHARED_<ID>: SharedRegion<bytes> = SharedRegion::new();` per region
      (id uppercased, non-alphanumeric folded to `_`). The runtime
      `SharedRegion<const N>` (`packages/core/nros-orchestration/src/lib.rs`)
      is a const-constructible zero-init `UnsafeCell<[u8; N]>` whose
      single `with(|&mut [u8; N]|)` accessor relies on the executor's
      cooperative single-thread dispatch (access discipline, not a lock —
      a future preemptive executor wraps it in the platform critical
      section). A component overlays its own typed view onto the bytes.
      Tests: planner collect filter/merge, generator render output +
      empty-renders-nothing, runtime static zero-init + mutate.

### Group 4 — Host tooling & DX

**Parallel lane.** Host CLI only — reads existing artifacts, touches no
runtime code or plan schema, so it is fully independent of Groups 1–3.
The three items are independent of each other.

- [x] **172.D — Incremental / staleness-aware build.** Skip
      regeneration + recompilation when the plan + sources are
      unchanged. Today `nros build` regenerates the package every
      run. Scope: content-hash the plan + component metadata; gate
      `generate_package` + the cargo invocation on staleness.
      **Landed** — `build_generated_package`
      (`packages/nros-cli-core/src/orchestration/build.rs`) now
      fingerprints the *generation* inputs (generator version + plan
      bytes + the paths baked into the manifest/build-script:
      `package_name`, `workspace_root`, `component_workspace`) with a
      `DefaultHasher` digest, records it in a `.nros-build-stamp` under
      the generated package root after a clean generation, and skips
      `generate_package` entirely when the stamp matches and the crate
      is present (printing "generated package up to date … skipping
      regeneration"). `nros build --force` / `NROS_BUILD_FORCE=1`
      bypasses the gate. **Recompilation is owned by cargo, not
      re-implemented:** the generated crate path-depends on the
      component crates, so cargo's own incremental fingerprinting is
      the authority on component-source staleness — `nros build`
      always invokes cargo (a no-op in ~0.06 s when nothing changed)
      rather than gate it on the plan hash, which would ship a stale
      binary whenever component source changed under an unchanged
      plan. The generator version is in the fingerprint so a CLI
      upgrade re-generates even on a byte-identical plan. Verified:
      unit tests for the fingerprint's input-sensitivity + the
      freshness predicate; the `orchestration_e2e` build test asserts
      the stamp is written; a real rebuild prints the skip line + cargo
      no-ops, and `--force` regenerates.

- [ ] **172.E — Hardened metadata-mode sandboxing.** The
      `nros metadata` mode compiles + runs component code to
      extract source metadata. Harden that execution (resource
      limits, filesystem/network restrictions) so untrusted
      component crates can't escape during metadata extraction.
      **DRIVER LANDED 2026-05-28; sandbox still deferred.** The
      metadata-mode *driver* (the thing this item must sandbox) is now
      implemented — `orchestration/metadata_build.rs`
      `build_metadata()` generates a tiny host harness (path-deps the
      component + `nros[std]`), `cargo run`s it; the harness runs
      `Component::register` against the in-memory `MetadataRecorder`
      (no transport/RTOS) and serializes via `to_source_metadata_json`.
      Verified by a real `orchestration_e2e` test building `demo_pkg`'s
      metadata. This unblocks real `nros metadata` / `nros deploy`
      end-to-end. **The sandbox hardening (this item) remains open** —
      it wraps the `cargo` invocation in `build_metadata`. The original
      deferral analysis (now resolved by the driver) follows.
      **DEFERRED 2026-05-27 — blocked on the driver.** Investigation
      (2026-05-27): there is nothing to sandbox yet. `nros metadata`
      (`cmd/metadata.rs`) only *discovers* the workspace, checks each
      declared component produced its `source-metadata.json`, and
      validates + copies it — it compiles/runs nothing (the
      `orchestration_e2e` fixture's `talker.metadata.json` is
      hand-written). The "compile each component in a host-side
      metadata mode and invoke its entry path with a fake
      `ComponentContext`" step (`docs/design/ros2-user-workflow.md`)
      — build component in metadata mode → run a harness that calls
      the macro-exported `__nros_component_register` against the host
      recorder → emit JSON — is **not implemented in `nros-cli-core`**
      (only the export glue exists: `nros-macros` →
      `__nros_component_register` + `__NROS_COMPONENT_EXPORT_PRESENT`,
      host recorder in `nros/src/component.rs`). Hardening a
      non-existent execution step is premature, so 172.E waits on that
      driver. **Design notes for when it lands** (so the work is
      pre-thought): untrusted code runs at *two* moments — compile
      (`build.rs` + proc-macros, inherent to `cargo build`) and run
      (`register()` + module static ctors); the compile-time vector is
      the elephant, so any real sandbox must wrap the whole `cargo
      build`, not just the harness exec. Recommended layered shape: a
      `sandbox` module wrapping the build+run `Command` — always-on
      `setrlimit` (CPU/AS/fsize/nproc, core=0) + env allowlist via
      `pre_exec`; an opt-in `strict` level (`--sandbox=off|limits|strict`
      / `NROS_METADATA_SANDBOX`) that prefixes the invocation with
      `bwrap --unshare-net --ro-bind <ws> --ro-bind <registry> --tmpfs
      <target> --die-with-parent`, degrading loudly (error, never
      silent) when `bwrap` is absent. Linux-first (namespaces/Landlock
      are Linux-only; macOS gets rlimits only). Host already has
      kernel 6.8 + `bwrap` 0.6.1 + rootless userns.

- [x] **172.F — Polished `nros explain`.** A user-facing command
      that explains the generated plan: which launch node maps to
      which component, how params resolved, why a SchedContext was
      chosen, what each generated table contains. Scope: a
      readable, structured rendering of `nros-plan.json` + the
      generation trace. **Landed** — `nros explain [plan]`
      (`packages/nros-cli-core/src/cmd/explain.rs`, default
      `build/nros/nros-plan.json`). Read-only: deserializes the same
      `NrosPlan` schema `nros check` validates, touches no runtime
      code or schema. Renders, in order: system header + generation
      trace (`generated by` / `system config` / `launch record`),
      build target, components, instances (launch-instance→component
      map → nodes → endpoints with interface + QoS
      reliability/durability/history(depth) → resolved parameters with
      `value [source-kind @ artifact]` → callback→context sched
      bindings + remaps), the SchedContext table (class / prio /
      period / budget / deadline(policy) / core / task), transports
      (bridge mode), and lifecycle / callback-chain / callback-group
      summaries when present. `render<W: Write>` is split out so the
      `orchestration_cli` fixture captures and asserts the rendering
      off the real metadata→plan artifact.

### Group 5 — Revised deployment model (ACTIVE, no backward compat)

**The single active direction** (other tracks paused). Implements the
*Revised deployment model* (2026-05-28): how generated wiring ships
across the three build-ownership models (self / vendor-lib /
vendor-module). Reuses the plan IR + planner + runtime features (Groups
1–4) unchanged.

**No backward compatibility — what this replaces (deleted, not kept
alongside):**
- the per-platform generated **`main`** (`EntryKind` / `render_main` in
  `generate.rs`) → the two-form **entry lib** (WP-B);
- the flag-driven **`nros build --launch/--system-plan/--system-output/
  --target/--rmw/…`** system interface → **`nros build|deploy <name>`**
  reading the root `nros.toml` (WP-A);
- the per-package **`system nros.toml` with `target.{triple,board}`** →
  the **root `nros.toml`** SSOT + `[deploy.<name>]` (WP-A).

These are removed in the item that supersedes them; no dual-read, no
compat shim. Direct-mode **component/example** `nros.toml` (172.K) is a
*different scope* (a self-contained single-node project) and is
unaffected.

**Work packages (3 large, coherent units).** Coarse-grained on purpose:
each package is one owner's end-to-end responsibility, not a pile of
tickets. Only **WP-A and WP-B are inherently parallel** (different
subsystems — host CLI vs generator); **WP-C is inherently sequential**
(the cutover needs both) and is kept whole.

```
   WP-A  Config & host CLI       ┐
                                 ├─ parallel ─┐
   WP-B  Generator (entry lib)   ┘            ├─▶  WP-C  Platforms & cutover
        (agree the interface at kickoff)      ┘        (after A + B land)
```

**Interface to agree at kickoff** (a short note, not a work package): the
entry-lib **C ABI** (`nros_<sys>_register_all(exec)`, per-node
`register_<node>`, `build_executor`, `Config` + the optional-override
entrypoint), the **`[deploy.<name>]` schema** (kind / target / board /
rmw / self / vendor.{dir,pin} / build[] / package[]), and the **runner
var-set** (`{self}` / `{entry_lib}` / `{entry_src}` / `{entry_header}` /
`{board}` / `{target}` / `{vendor.dir}`). WP-A builds against it stubbing
the lib emit; WP-B builds against it stubbing the runner; they meet when
both land.

##### Kickoff interface — pinned 2026-05-28 (WP-B owner)

Concrete contract both packages build against. `<sys>` is the system
name, lowercased with non-alphanumerics → `_` (same rule as
`SHARED_<ID>`). The entry lib exposes **both** a Rust-native surface
(for `self`'s generated thin shim — no FFI cost) and an identical C ABI
(for vendor startup in C/C++/`make`/`west`). The C ABI reuses
`nros-c`'s opaque-`Executor` handle convention (Phase 118) rather than
inventing one — same storage/owning model, so vendor code that already
links `nros-c` sees a familiar type.

**Entry-lib C ABI** (compiled + source forms export identical symbols):

```c
typedef struct NrosExecutor NrosExecutor;   // opaque, as in nros-c
typedef struct NrosConfig   NrosConfig;     // optional runtime override

// Build the executor (opens the RMW session(s)); cfg = NULL ⇒ baked/env
// config (precedence: param > env > baked). NULL return ⇒ error.
NrosExecutor *nros_<sys>_build_executor(const NrosConfig *cfg);
// Register sched contexts + every node + lifecycle + param persistence.
int32_t nros_<sys>_register_all(NrosExecutor *exec);
// Granular per-node registration (register_all calls these in plan order).
int32_t nros_<sys>_register_<node>(NrosExecutor *exec);
// Convenience blocking spin for vendor startup that just wants to run.
int32_t nros_<sys>_spin(NrosExecutor *exec);
void    nros_<sys>_destroy(NrosExecutor *exec);
```

Rust-native mirror (same module, used by `self`'s shim):
`pub fn build_executor(cfg: Option<&Config>) -> Result<Executor>`,
`pub fn register_all(exec: &mut Executor) -> Result<()>`. The C ABI
functions are thin `#[unsafe(no_mangle)] extern "C"` wrappers over these;
cbindgen emits `<sys>.h` (config committed in the generated crate,
mirroring `nros-c`'s `cbindgen.toml`).

**`[deploy.<name>]` schema** (root `nros.toml`):

| key | type | notes |
|---|---|---|
| `kind` | `self` \| `vendor-lib` \| `vendor-module` | default `self` |
| `target` | string | triple or platform token; required |
| `board` | string | required for vendor-module / bare-metal |
| `rmw` | string | optional; overrides `[system].rmw` |
| `emit` | `compiled` \| `source` | default by kind: self/vendor-lib → `compiled`, vendor-module → `source` |
| `self` | path | code dir (`deploy/<name>/`); startup shim / vendor shell |
| `vendor.dir` | `{ env = "VAR" }` \| path | vendor SDK root |
| `vendor.pin` | string | `"<name> <version>"`, asserted before build |
| `build` | string[] | ordered build steps (vendor shell lines) |
| `package` | string[] | ordered post-build artifacting |

**Runner var-set** (substituted into `build[]` / `package[]`):
`{self}` (abs `deploy/<name>/`), `{entry_lib}` (compiled: `lib<sys>.a`),
`{entry_src}` (source: generated crate dir), `{entry_header}`
(`<sys>.h`), `{board}`, `{target}`, `{vendor.dir}` (resolved).

WP-A builds the runner/scaffolders against this; WP-B emits the lib +
header + source CMake fragment exporting exactly these symbols.

#### WP-A — Config & host CLI  *(parallel with WP-B; was 172.O + Q + T)*

Owns the `nros.toml` config scope + the `nros` command surface. Files:
`nros-cli-core` orchestration loader + `cmd/{check,deploy,build,new}.rs`.

- **Root `nros.toml` SSOT.** The workspace-root config (marked by
  `[workspace]`, distinct from per-package `nros.toml`): `[workspace]`
  (`default`), `[system]` / `[systems.<name>]` (launch path, component
  refs, default `rmw` + `domain_id`, `[overlays.*]`,
  `[[domain]]` / `[[bridge]]`), `[deploy.<name>]` tables. Loader + schema +
  `nros check` validation. **Deletes** the per-package "system
  `nros.toml` with `target.{triple,board}`" reader — triple/board live in
  `[deploy.<name>]`. Component `nros.toml` stays optional (reusable
  intrinsics) and must not carry `rmw` / `domain`.
- **`nros deploy` / `nros build <name>` command-runner.** Assert
  `vendor.pin` → emit the entry-lib form → run `build[]` → `package[]`,
  substituting the var-set. Three kinds (`self` / `vendor-lib` /
  `vendor-module`), **no per-vendor code** (steps are user shell lines).
  Pin-drift `nros doctor` check; `package[]` owns post-build artifacting
  (mkifs / sign / `.px4`). **Deletes** the flag-driven system-build
  interface (`--launch` / `--system-plan` / `--system-output` /
  `--target` / `--rmw` ad-hoc); plain single-crate `nros build` autodetect
  stays.
- **Scaffolders + eject gradient.** `nros new --deploy <name> --kind <k>
  --target <t>` scaffolds `deploy/<name>/` + appends a `[deploy.<name>]`
  table to root; `--from-launch` / `--from-profile`. Implicit `nros build`
  → `[workspace].default`; `nros build <name>` resolves a profile or a
  `deploy/<name>/` dir (one namespace) — no long commands.

#### WP-B — Generator: entry lib, config lowering, bridges  *(parallel with WP-A; was 172.P + R + S)*

Owns everything the generator emits. Files: `orchestration/generate.rs`
(+ planner for bridges), cbindgen. The owner may modularize `generate.rs`
into submodules if it helps — their call, not a mandated step.

- **Two-form entry lib + delete `render_main`.** Emit the wiring as a
  **library** with a granular C ABI, in two forms: **compiled**
  (`lib<sys>.a` + cbindgen `<sys>.h`) and **source** (crate + a
  vendor-includable CMake fragment via `add_subdirectory` + corrosion);
  `[deploy].emit` / kind selects, vendor-owns-toolchain → source.
  **Delete** the `EntryKind` / `render_main` per-platform `main` emitter —
  there is no generated `main`; platform startup is deploy-side (self's is
  a thin generated shim). Re-express the Zephyr staticlib path as the
  unified source-form lib.
- **Config lowering.** Lower the plan's config (domain, locator,
  transport, params) per `(net_owner × host/embedded)`: host → env at
  runtime; embedded + NanoRosOwned → baked board `Config` + shared
  `<sys>_config.h`; embedded + RtosOwned → domain/locator baked + NIC
  config as a vendor Kconfig/defconfig fragment in `{self}`; vendor-link
  (IVC) → channel in the shared header; uORB → params baked. C-ABI entry
  takes an optional `Config` override; precedence **param > env > baked**.
  Builds on 172.J + Phase 173.7 + the `NetStack` enum.
- **Bridges + multi-domain** (subsumes 172.K.5). Node = one session
  (rmw + domain); bridge spans sessions, not a node. OUT bridge = runtime
  `nros-bridge.toml` (172.L, unchanged) as its own deployable. IN bridge =
  build-time `[[bridge]]` / `[[domain]]` in root `nros.toml` →
  planner/generator emit `Executor::open_multi([SessionSpec])` + per-node
  `create_node_on` session assignment (this *is* the old K.5 by-id
  binding). `build.rmw` becomes a *set* for in-binary cross-RMW;
  `nros check` warns when a target can't link the set.
  *Landed:* the RMW-set feasibility warning (`nros check`); the SESSION_SPECS
  emission (`Executor::open_multi`, ≥2-transport bridge plans); and
  **multi-domain session opening** — `PlanTransport` gained `domain` and
  SESSION_SPECS now emits `SessionSpec::new(rmw, locator).domain_id(d)`, so a
  bridge opens same-rmw sessions on distinct domains. *Blocked (the per-node
  routing half):* binding each node to its session needs the
  `[[bridge]]`/`[[domain]]` → planner → plan chain — `PlanInstance`/`PlanNode`
  carry no transport/session binding yet (nodes have `domain_id` only), so the
  generator can't route `build_component_node` via `.rmw(session)`/the K.5
  by-id binding. That mapping is planner work consuming root_config's
  `BridgeSpec`/`SystemComponent.transport` (WP-A-coupled); the runtime
  primitives (`create_node_on`, `SessionSpec.domain_id`) are already in place.

#### WP-C — Platforms & cutover  *(sequential, after WP-A + WP-B; was 172.V + U)*

Make the model real across platforms, then flip `main`. Kept whole — the
steps are inherently ordered (templates → migrate → delete). Files:
`deploy/<vendor>/` templates, `integrations/` re-home, fixtures, book.

- **Per-platform `deploy/<vendor>/` templates.** One template per platform
  — self/posix, bare-metal, freertos, nuttx, threadx, zephyr, esp-idf,
  px4, orin-spe, qnx — each the kind-specific shell (link line /
  `add_subdirectory` fragment / vendor-module manifest) + example
  `build[]` / `package[]` + config-fragment hook + a sim/HW validation.
  Re-home the existing `integrations/<rtos>/` shells (Phase 139) as these.
  The owner sequences the platforms (or sub-delegates if staffed); the
  roadmap does not pre-split them into separate tickets.
  *In progress:* the **vendor-lib** template landed
  (`examples/templates/deploy/vendor-lib/` — `startup.c` driving the WP-B
  `nros_<sys>_*` C ABI + a README with the `[deploy]` table / `build[]` link
  line), validated host-side by a `nros deploy --dry-run` e2e (resolve +
  var-set substitution). The remaining platforms (self/posix [generated
  shim, no dir], bare-metal, freertos, nuttx, threadx, zephyr, esp-idf, px4,
  orin-spe HW, qnx) + each one's real sim/HW build validation are SDK/HW-bound
  and pending an environment with those toolchains.
- **Migrate + flip.** Convert the `orchestration_e2e` fixtures + at least
  one self, one vendor-lib (Orin POSIX sim), one vendor-module (Zephyr or
  PX4-SITL) system to a root `nros.toml` + `nros deploy <name>`. **Then
  delete the superseded code** (the `render_main` remnants WP-B left, the
  old `cmd/build.rs` system flags + tests, the per-package triple/board
  reader). Update the book (`ros2-user-workflow`, `configuration`, CLI
  ref). Done when `grep` shows zero references to the deleted surfaces,
  `just ci` is green, and the three sample systems deploy from one root
  file.
  *Done (native `self`):* the `orchestration_e2e` fixture carries a root
  `nros.toml` (`[workspace]`/`[system]`/`[deploy.native]`) + `demo_pkg`
  declares its component (`component_nros.toml`), and
  `deploy_native_self_from_root_nros_toml` proves **`nros deploy native`
  builds the self-shim end-to-end from one root file** —
  metadata (auto-collected from the declaration, WP-A's deploy auto-build
  `6bdd945`) → plan → compiled entry lib → self-shim binary. Enabling fixes:
  the generated `Cargo.toml` gained an empty `[workspace]` (standalone when
  emitted in-tree under a deploy build dir), and `schema_components` dedups by
  component id (its metadata reaches the planner from both the build dir and
  the in-package `metadata/` file). *Deletion half DONE (the flip, 2026-05-28,
  `2dab9e4`→`9bc3e64`):* the entry lib was generalized to **every** non-bridge
  platform (hosted `fn main` + no_std board shim + nuttx/orin route + the
  Zephyr staticlib fold), `render_main` was deleted, the bridge path folded
  onto `build_executor_bridge`, and the dead `SystemConfig`/`TargetConfig`
  triple/board reader was retired. `EntryKind` survives only to pick the shim
  *shape* (hosted vs no_std board vs Zephyr staticlib), not to emit a
  per-platform `main`. (The flip regressed three `orchestration_generate`
  assertions to the pre-flip shim shape; fixed in `307a87c`.) *Build-flag
  retirement DONE (2026-05-28, codegen `01c8512`):* the legacy
  `cmd/build.rs --launch` one-shot and `--system-plan` pre-planned-build flags
  (+ their orphaned `--system-pkg`/`--metadata`/`--manifest`/`--launch-arg`/
  `--out-dir`/`--system-output`/`--system-package`/`--release`/`--force`/
  `--target` siblings + the `infer_package_name` helper) are gone — `nros
  build` is now deploy-dispatch ∪ project-flavor only. System-from-plan is
  strictly an orchestration-library call: the 12 `orchestration_e2e`
  cross-build tests invoke `build_generated_package(&BuildOptions{..})`
  directly, and the `--launch` one-shot e2e (fully covered by
  `fixture_workspace_plans_checks_and_builds_generated_package`) was dropped.
  `grep` shows zero `--system-plan`/`--launch` references outside archival
  roadmap docs + the CLI ref's retirement note. *Still pending:*
  the vendor-lib / vendor-module **sample deploys** — the vendor-lib template +
  dry-run runner landed (172.V); a real cross-compile build is SDK-bound (W.4).

**Re-evaluated under the model:** **172.K.7** (multi-homing
`[[transport]].interfaces`) is transport-schema work orthogonal to
deployment — carries forward as-is. **172.E**: its *driver* (metadata-mode
build+run) landed 2026-05-28 (`metadata_build.rs`), unblocking real
`nros metadata`/`nros deploy`; the *sandbox* hardening stays open,
independent of Group 5.

### Revision (2026-05-28, post-review)

A workflow/UX review (self-model proven on QEMU/native; vendor models
designed + scaffolded but not yet real-built) raised four items; design +
the cheap fix landed, the rest are tracked here:

- **W.1 — Cargo-style manifest resolution. DONE** (2026-05-28, codegen
  `1ff8a51` fold + `685354e` probe/walk-up). **Design canonical**
  (`docs/design/configuration-and-transports.md`, "Manifest kinds &
  resolution"): one `nros.toml` schema, kind decided by sections present
  (`[workspace]` / `[component]` / `[node]`, combinations allowed like
  Cargo's root package), walk-up resolution, `component_nros.toml` folds
  into a `[component]` table (deprecation window). *Landed:* the
  section-discriminating probe (`ManifestKind` + `probe_manifest_kind`,
  `[workspace]` dominates), the walk-up resolver (`resolve_workspace_root`),
  the `component_nros.toml`→`[component]` fold (`workspace::load_component_config`
  reads either form; legacy warns once; discovery prefers folded, dedups by
  `(package, component)`), and the kind-specific "direct-mode node / detached
  component, not a workspace" error on `nros deploy`/`build`. *Not done:* the
  optional `config.rs` `ComponentConfig` rename (cosmetic — skipped); the
  `component_nros.toml` deprecation *removal* waits out the window.
- **W.2 — bridge schema-ahead-of-impl.** DONE — `nros check` on a root
  `nros.toml` warns when `[[bridge]]` is declared but per-node bridge routing
  isn't emitted (`cmd/check.rs` `pending_routing_warning`). Narrowed to
  `[[bridge]]` only once 172.K.5 landed `[[domain]]` multi-domain routing
  (2026-05-28); fully removed when bridge topic-forwarding lands.
- **W.3 — `[component]` UX. DONE** (codegen `d5d6382`+`f1fc4f4`+`51cb0d0`).
  *Landed:* `#[serde(default)]` on `ComponentOverrides` + its
  `parameters`/`remaps` *and* the whole `[linkage]` table + `ComponentConfig.
  overrides`/`linkage` (a minimal `[component]` = package + component +
  language + metadata parses, no more *"missing field `parameters`"*);
  `metadata --build` derives `linkage` via `ComponentLinkage::resolved_*`
  (executable ← component short name, exported_symbol ← `nros_component_<name>`,
  crate_name ← package with `-`→`_`); and `nros new --component <name>
  [--use-case <c>]` scaffolds a planned-mode component — an `nros::Component`
  lib (`pub mod <use_case> { struct Component }`, registered via the Rust type
  path) + a folded minimal `[component]` `nros.toml` (no `[linkage]`/
  `[overrides]`, `crate::module` id). `nros new` keeps emitting the direct-mode
  `[node]` manifest for plain binaries — `--component` is the orchestration
  counterpart, so the two manifest kinds match the design's section model.
- **W.4 — validation reach.** *Entry-lib generalization DONE* (the flip,
  above): every non-bridge platform now routes through the entry lib, so
  `render_main` is gone and the `self` model is structurally uniform across
  hosted + board + Zephyr. *Still:* end-to-end **real builds** are proven only
  for `self` on QEMU/native; vendor-lib (Orin) is mock-IVC, vendor-module
  (Zephyr/PX4/…) is shape-only. Prioritize one real vendor-module
  (Zephyr `west` or PX4-SITL) + one real vendor-lib link before claiming
  the three-ownership-model workflow is proven on hardware.
- **W.5 — board-scoped first-image setup → split out to Phase 187.** The
  toolchain/SDK-distribution work (the first-image UX delta: `just setup` pulls
  all platform SDKs ≈ 7.4 GB incl. a 2.7 GB QEMU source build, vs a board-scoped
  prebuilt fetch) is a distinct concern — *dependency/toolchain management +
  distribution infra* (`nros setup`, a versioned package index + lockfile,
  GitHub-Releases hosting, a CI bump→release gate), orthogonal to orchestration.
  Tracked in **`docs/roadmap/phase-187-toolchain-sdk-distribution.md`**; designed
  in `docs/design/nros-setup-toolchain-management.md`.

Each work item is independently shippable. A work item is done when:

- [ ] Its capability is represented in `nros-plan.json` (where it
      affects the plan) with round-tripping fixtures.
- [ ] `nros check` validates the new construct.
- [ ] The generated runtime exercises it, verified by an
      `orchestration_e2e` fixture (or a unit test where no generated
      binary is involved).
- [ ] Docs show the workflow for the new capability.

Group 1 (configuration) additionally:

- [ ] A project carries at most `Cargo.toml` **or** `CMakeLists.txt`
      (build), `package.xml` (identity + msg deps), `.cargo/config.toml`
      (patch injection only), and one `nros.toml` (all nano-ros config).
      No `config.toml`; no `nros.toml`/bridge name clash.
- [ ] A single-node example builds + boots in **direct mode** from
      `nros.toml` (network + RMW + RT) with no launch file or generated
      `main`, on both a hosted and an embedded (`include_str!`) target.

Group 5 (deployment model) additionally:

- [ ] All deployment + system config lives in **one root `nros.toml`**;
      `deploy/<name>/` holds only code/templates referenced by path. RMW +
      domain have a single SSOT (`[system]`, with `[deploy]`/host-env
      override following the documented precedence); component `nros.toml`
      carries neither.
- [ ] One system deploys to **≥2 ownership models** from the same root
      config via `nros deploy <name>` — e.g. a `self` native binary and a
      `vendor-module` build — each a single command, no long flags, the
      vendor build sequenced by the command-runner with no per-vendor code
      in nano-ros.
- [ ] The entry lib builds in **both forms** (compiled `.a` + header;
      source + corrosion-compiled by a vendor toolchain), exercised by an
      `orchestration_e2e` fixture per form.

## Notes

- Items keep their stable `172.<letter>` IDs from when A–I were Phase
  126's "Deliberate deferrals" and J–N were absorbed from Phase 116
  (archived). The groups re-cluster them by area, not by origin — e.g.
  scheduling item 172.G (originally a 126 deferral) now sits with the
  planner items 172.B/C.
- **Cross-group parallelism is the point**; pick groups by available
  hands. Group 4 (host tooling) is the most independent. Within groups,
  the lowest-risk single wins are 172.L + 172.M (Group 1) and 172.D +
  172.F (Group 4); the heaviest are 172.K (88 examples + 86
  `include_str!` + 8 board parsers, Group 1) and 172.A / 172.G (Groups
  3/2).
- Groups 1–3 share the `nros-plan.json` schema — coordinate additive,
  version-bumped changes; don't let two groups mutate the schema in the
  same window without rebasing.
- **Group 5 (WP-A/B/C)** builds on the now-complete planning half (Groups
  1–4): it reuses the plan IR and changes only how the generated wiring
  is *shipped*. WP-A owns a new config scope (root `nros.toml`) + the
  `nros deploy` surface; WP-B touches `generate.rs` (entry-lib forms +
  in-binary bridges in the planner), so coordinate those with any late
  Group 2/3 work. **Decided 2026-05-28:** ejected deployment code lives
  under a top-level **`deploy/<name>/`** (not `src/<name>/`) — it is
  deployment glue, not an application package, and keeping it out of
  `src/` stops colcon from treating vendor module shells as buildable
  workspace packages. **Open fork** (WP-B bridges): whether an
  in-workspace bridge always anchors its own deployable vs. living in a
  normal app system's root config — leaning "its own deployable" to
  keep the node = one-session invariant + RMW-set feasibility analysis
  clean.
