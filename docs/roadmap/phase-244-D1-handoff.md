# Phase 244.D1 — qemu-arm-baremetal Rust migration — handoff checkpoint

**Status 2026-06-13.** Handoff for the agent continuing D1. D1 routes every
qemu-arm-baremetal Rust example through `nros::main!()` (Node-pkg + Entry-pkg
split) per RFC-0024 + RFC-0032. This doc captures what landed, what is left, the
enablers built, and the known deviations — so the rest can proceed without
re-deriving the design. (Kept separate from `phase-244-…-cleanliness.md` to avoid
edit conflicts with the parallel D2/D3/D4/D6 effort.)

## Landed (on `main`)

### Enablers (Wave-0, reusable)
1. **qemu-mps2-an385 BoardEntry** (`d53fab463`, `74df8b0a2`) — `impl
   nros_platform::BoardEntry for Mps2An385` behind the board's `board-entry`
   feature (`nros-board-mps2-an385/src/entry.rs`); macro maps `qemu-mps2-an385`
   → `Mps2An385` and emits a `#[cortex_m_rt::entry]` reset for pure bare-metal
   Cortex-M (vs FreeRTOS's `extern "C" fn main`), keyed by
   `is_baremetal_cortexm_deploy` (`nros-macros/src/main_macro.rs`). Proven: links
   thumbv7m + boots under QEMU — test
   `nros-tests::baremetal_run_plan_runtime::baremetal_board_run_executes_run_plan`
   (locates the prebuilt `qemu-baremetal-main-e2e` fixture; needs
   `just qemu-baremetal build-fixtures`).
2. **nros_log dispatcher in BoardEntry boot** (`753c1cd5c` / equiv) — `boot()`
   calls `nros_log::init(sinks::default())` after `init_hardware`, so declarative
   nodes `nros_info!` without a per-example boot closure. Nodes still
   `register_logger(&LOGGER)` in `register()`.
3. **RTIC deploy-net** (`cf6b3ae3c`) — non-breaking defaulted
   `RticBoardEntry::init_hardware_with_deploy(device, core, &DeployOverlay)`; the
   macro's RTIC `#[init]` calls it; `RticMps2An385` overrides it
   (`qemu_config_with_overlay`) so each RTIC Entry pins its own ip via
   `[deploy.rtic-mps2-an385]`. Required so the talker/listener pair don't collide
   on the board's baked ip. (NOTE: `DeployOverlay` has **no MAC key** — the pair
   shares the baked MAC; harmless since each QEMU slirp net is isolated.)

### Migrations
- **Wave A** (`9bf788b7b`): `talker`, `listener` → Node+Entry on the
  qemu-mps2-an385 OwnedSpin enabler. `[deploy.qemu-mps2-an385]` net block.
- **Wave B** (`168a1ed22`): `talker-rtic`, `listener-rtic`, `talker-rtic-mixed`,
  `listener-rtic-mixed` → Node+Entry on the RTIC board, `[deploy.rtic-mps2-an385]`.

All migrated examples **build clean for thumbv7m**; output markers
(`Published:`/`Received:`) preserved; `[[bin]]` names unchanged.

## Migration recipe (proven — follow for the rest)
- **Node pkg** `<ex>_pkg/`: `[lib] crate-type=["rlib"]`, `#![no_std]`, declarative
  `impl Node` (create publisher/sub/timer + `register_logger`) + `impl
  ExecutableNode` (publish/handle + `nros_info!` with the EXACT old marker),
  `nros::node!`. **Move** the old example's `generated/` + `package.xml` +
  `[patch.crates-io]` msg entries here. `[package.metadata.nros.node]` class/name/
  dispatch. No Executor/Config/rmw-register/platform headers.
- **Entry pkg** (existing dir): `src/main.rs` = the 4-line
  `#![no_std]/#![no_main]/use panic_semihosting as _;/nros::main!();`.
  - **OwnedSpin (qemu-mps2-an385, serial)**: needs `src/lib.rs` re-exporting
    `<ex>_pkg::register` + an empty `launch/system.launch.xml` (Form-1
    self-bringup). `[entry] deploy="qemu-mps2-an385"`. board dep `features=["board-entry"]`.
  - **RTIC (rtic-mps2-an385)**: NO lib.rs, NO launch — `[entry]
    node_pkgs=["<ex>_pkg"]` drives the node set. board dep
    `nros-board-rtic-mps2-an385` + `rtic`/`cortex-m`/`mps2-an385-pac`.
  - `[package.metadata.nros.deploy.<board>]` block with the old `Config` net
    (locator/ip/gateway/netmask). **Keep `[patch.crates-io]` in the ENTRY**
    pointing at `../<ex>_pkg/generated/*` — cargo honors `[patch]` only from the
    build root.
- Compile-check: `cargo build --release --target thumbv7m-none-eabi` from the
  Entry dir (after `source ./activate.sh`).

## Remaining

### Wave C — service/action -rtic (×4) — BLOCKED on an enabler
`service-{client,server}-rtic`, `action-{client,server}-rtic` use imperative
`create_service`/`create_action_*` + RTIC poll tasks. The declarative
service/action dispatch (Phase 212.M-F.23) lives **only** in OwnedSpin's
`ExecutorNodeRuntime` (`nros/src/node_runtime.rs`); the RTIC `RticRuntime`
(`nros-board-rtic-mps2-an385/src/lib.rs`) has **no** service/action handling.
**Needed first:** declarative service/action dispatch on the RTIC runtime
(M-F.23-for-RTIC). Then migrate the 4 with the recipe above.

### Wave D — serial (×2) — ready (no new enabler)
`serial-talker`, `serial-listener` are pub/sub over UART
(`run(Config::serial_default(), …)`). Migrate to `nros::main!()` +
`deploy="qemu-mps2-an385"` (OwnedSpin enabler) + the board's `serial` feature
(Entry: `nros-board-mps2-an385 = { features=["board-entry","serial"] }`,
default ethernet off). Deploy locator `serial/UART_0#baudrate=115200`. Same Node+Entry
recipe as Wave A. PTY wiring: QEMU `-serial pty` ↔ zenohd serial plugin (see the
old `serial-talker/src/main.rs` doc-comment + `emulator::test_qemu_serial_pubsub_e2e`).

### Not D1
`talker-xrce` is **D4** (custom-transport, `nros-transport-callbacks`) — owned by
the parallel effort, not this wave plan.

## Verification status / known gaps
- **thumbv7m compile**: all migrated examples GREEN.
- **QEMU boot**: proven for the OwnedSpin path (e2e fixture test).
- **Full pub/sub message-flow E2E** (`emulator::test_qemu_bsp_pubsub_e2e` /
  `test_qemu_rtic_pubsub_e2e`): **NOT confirmed locally** — both firmwares boot +
  apply the deploy ip but fail at `Executor::open: ConnectionFailed`, a
  zenohd-slirp reachability issue in the dev env (these pass in CI). Re-run in CI
  to confirm. The declarative pub/sub pattern itself is proven over real zenoh by
  `nros-tests::component_dispatch`.
- **RTIC priority-task split NOT preserved** — the legacy RTIC apps ran
  net_poll + publish/listen as separate (priority-1) async tasks; the declarative
  node collapses to the macro's single deferred dispatch task. Pub/sub behavior +
  markers intact; the `-mixed` tasks were all priority-1 so no real mixed-priority
  semantics were lost. If a future example has genuinely distinct priorities,
  `nros::main!(custom_tasks = …)` (E1) is the escape hatch.
- **Pre-existing warnings** in `nros-board-rtic-mps2-an385` (unused
  `MaybeUninit`/`PublisherResolver` when `e2e-synthetic-callback` is off) — not
  introduced by D1.
- **rtic-monotonics dropped** from migrated RTIC entries (the macro RTIC app
  drives timing via the declarative executor timer, not a SysTick Mono — matches
  the `phase216-rtic-e2e` template).

## Acceptance (phase close, unchanged)
Issue-0049 rubric → 0 `major`; each platform's `build-fixtures` + E2E green;
update issue 0049 → resolved.
