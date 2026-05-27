# Phase 172 — Orchestration follow-ups (deferred from Phase 126)

**Goal.** Land the capabilities that Phase 126 (ROS 2 workflow
orchestration MVP) explicitly deferred, **plus the configuration
consolidation absorbed from Phase 116** (archived 2026-05-27). Phase
126 shipped the end-to-end MVP — source metadata → launch plan →
checked `nros-plan.json` → generated per-board binary, verified across
9 boards. This phase organizes the remaining work into **four parallel
work groups** (see below).

**Status.** Groups 1–4 = **completed planning foundation** (L, M, J,
K.1–K.6, N, B, C, G, A, H, I, D, F). **Group 5 (revised deployment model)
is now the single active direction** (2026-05-28) — other tracks paused
while it lands. **No backward compatibility:** Group 5 *replaces* the
generated-`main` path, the flag-driven `nros build`, and the per-package
`system nros.toml`; superseded code is deleted, not kept alongside.

**Priority.** P2 — none block the MVP workflow; each is an
ergonomic or capability upgrade on top of a working pipeline.

**Depends on.** Phase 126 (archived) — the schema, planner,
checker, generator, and per-board templates this phase extends.

**Subsumes.** Phase 116 (configuration redesign, archived). See *Why
configuration lives here* below.

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
| **5 — Revised deployment model (ACTIVE, no-compat)** | root `nros.toml` SSOT, two-form entry lib, `nros deploy` command-runner, `deploy/` dirs, config lowering, bridges, eject gradient, per-vendor templates, migration + deletion | O, P.M, P, Q, R, S, T, V, U | **Group 1** contracts (O.1, P.0, Q.0, R.0, P.M) → **SYNC 1** → **Group 2** wide parallel (O.2/3, P.1/2, Q.1/2, R, S, T, V.\*) → **SYNC 2** → **Group 3** flip (P.3, U, V validation) → **SYNC 3**. See *Parallelization plan*. |

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
  - [ ] **172.K.5 — runtime `create_node_on`-by-id.** Bind a node to a session
        by transport `id` (not just `rmw`); only required for **case D**
        (segregated same-rmw sessions) in the transport taxonomy.
        **SUBSUMED by 172.S** (2026-05-28): per-node session assignment for
        in-binary multi-domain/bridge builds is exactly this binding —
        implement it there against the `[[domain]]`/`[[bridge]]` root config.
  - [ ] **172.K.7 — multi-homing `[[transport]].interfaces` (list).** A single
        session spanning several NICs as one merged graph (taxonomy cases B/C —
        the common "node reachable on multiple interfaces" need, what stock
        DDS/zenoh do natively). Generalize the current single `interface` field
        to a list; generator maps it per backend (zenoh listen/connect per NIC +
        scouting iface; Cyclone `<Interfaces>`; Fast DDS whitelist). Distinct
        from K.5 (merge vs segregate). Design:
        [`docs/design/configuration-and-transports.md`](../design/configuration-and-transports.md)
        ("Two axes" taxonomy).
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
  `generate.rs`) → the two-form **entry lib** (172.P);
- the flag-driven **`nros build --launch/--system-plan/--system-output/
  --target/--rmw/…`** system interface → **`nros build|deploy <name>`**
  reading the root `nros.toml` (172.Q);
- the per-package **`system nros.toml` with `target.{triple,board}`** →
  the **root `nros.toml`** SSOT + `[deploy.<name>]` (172.O).

These are removed in the item that supersedes them; no dual-read, no
compat shim. Direct-mode **component/example** `nros.toml` (172.K) is a
*different scope* (a self-contained single-node project) and is
unaffected.

**Parallelization plan (built for many hands).** The naive order
(O→P→Q→R‖S‖T→U) is mostly serial. Break it into **parallel groups
separated by sync barriers**: inside a group, tickets run concurrently
with no shared files (see the lane-ownership table); at each sync point
all of the group's work merges and must meet the barrier's exit criteria
before the next group starts. Two moves make the groups wide: **(1)
contracts first** (Parallel Group 1 freezes the interfaces), and **(2)
`generate.rs` modularized** (172.P.M) so the generator lanes own
different files.

```
branch off main
  │
  ├─ PARALLEL GROUP 1 — Contracts & enablers  (independent; distinct files/docs)
  │     O.1 root schema · P.0 C-ABI header · Q.0 runner var/step · R.0 lowering spec · P.M split generate.rs
  ▼
 ══ SYNC 1 "Contracts frozen" ══════════════════════════════════════════
  │   exit: schema round-trips; C-ABI header frozen + documented;
  │   generate.rs modular with all existing tests green; specs reviewed.
  │   Tag the contract commit — Group 2 rebases on it. (the critical barrier)
  │
  ├─ PARALLEL GROUP 2 — Build-out  (file-isolated lanes; as many hands as available)
  │     LANE config   O.2 check + O.3 loader            → cmd/check.rs, loader
  │     LANE deploy   Q.1 runner + Q.2 resolution       → cmd/deploy.rs, cmd/build.rs (stubs entry-lib until P)
  │     LANE entrylib P.1 compiled + P.2 source         → generate/entry_lib.rs
  │     LANE lowering R                                 → generate/config_lower.rs
  │     LANE bridge   S                                 → generate/bridge.rs + planner
  │     LANE scaffold T                                 → cmd/new.rs
  │     LANE vendor   V.* (~10, own parallel sub-group) → deploy/<vendor>/  (authoring only)
  ▼
 ══ SYNC 2 "Build-out integrated" ══════════════════════════════════════
  │   exit: every lane unit-green; ONE self system goes
  │   entry-lib → deploy → config-lower → boot end-to-end; vendor
  │   templates authored; render_main now unreferenced.
  │
  ├─ PARALLEL GROUP 3 — Flip & cleanup  (independent again)
  │     P.3 delete render_main · U:migrate-fixtures · U:delete-dead-paths
  │     U:docs (per book page) · V.* sim/HW validation (per platform)
  ▼
 ══ SYNC 3 "Model is the only path" ════════════════════════════════════
      exit: zero refs to deleted surfaces; `just ci` green; 3 sample
      systems (self / vendor-lib / vendor-module) deploy from one root
      file. Merge to main — the flip.
```

**Barrier discipline.** Each sync point is a hard merge gate, not a
suggestion: a lane that misses a barrier blocks only the *next* group,
not its peers (peers already merged). The two barriers that matter most
are **SYNC 1** (freeze before anyone builds against the contract —
re-opening it forces a Group-2-wide rebase) and **SYNC 3** (the flip —
nothing deletes a superseded surface until the new path is proven at
SYNC 2). The **172.V vendor matrix straddles Groups 2→3**: template
*authoring* is Group 2 (parallel, against the Q.0 contract); per-platform
*sim/HW validation* joins Group 3 (needs the runner + the platform's gen
lane).

**Lane ownership (conflict-free concurrency within a group).**

Column **Grp** = parallel group (1/2/3) from the fork-join above.

| Item | Grp | Owns (files) | Depends on | Runs parallel with |
|---|---|---|---|---|
| O.1 schema types | 1 | `orchestration/root_config.rs` (new) | — | P.0, Q.0, R.0, P.M |
| P.0 C-ABI header | 1 | `<sys>.h` template + ABI doc | — | O.1, Q.0, R.0, P.M |
| Q.0 var/step contract | 1 | deploy-runner spec | — | O.1, P.0, R.0, P.M |
| R.0 lowering matrix | 1 | lowering spec table | — | O.1, P.0, Q.0, P.M |
| P.M split generate.rs | 1 | `generate/mod.rs` + submodules (mechanical) | — | the other Wave-1 items |
| O.2 `nros check` rules | 2 | `cmd/check.rs` | O.1 | everything in Wave 2 |
| O.3 loader → plan/runner | 2 | `orchestration/{root_config,planner}` glue | O.1 | P/Q/R/S/T/V |
| P.1 compiled form | 2 | `generate/entry_lib.rs` | P.0, P.M | R, S, Q, T, V |
| P.2 source form | 2 | `generate/entry_lib.rs` (source path) + corrosion frag | P.0, P.M | R, S, Q, T, V (coordinate w/ P.1 in same file) |
| Q.1 runner | 2 | `cmd/deploy.rs` (new) | O.1, O.3, Q.0 | P, R, S, T, V |
| Q.2 resolution + strip flags | 2 | `cmd/build.rs` | O.1, Q.0 | P, R, S, T, V |
| R config lowering | 2 | `generate/config_lower.rs` | R.0, P.M, 172.J | P, S, Q, T, V |
| S bridges | 2 | `generate/bridge.rs` + planner bridge inference | O.1, P.M | P, R, Q, T, V |
| T scaffolders | 2 | `cmd/new.rs` + `deploy/` templates | O.1, Q.0 | P, R, S, Q, V |
| V.* vendor templates | 2 | `deploy/<vendor>/` (one dir each) | Q.0 (content) | each other + all of Wave 2 |
| P.3 delete render_main | 3 | `generate/render.rs` removal | P.1, P.2 | — |
| U flip + delete + docs | 3 | fixtures, dead-path removal, book | all | — |

- [ ] **172.O — Root `nros.toml` workspace config (SSOT).**
      *(Sub-items per the lane table: O.1 schema types (Wave 1) · O.2
      `nros check` rules · O.3 loader → plan/runner.)*
      Add the workspace-scope config file at the repo root (marked by
      `[workspace]`, distinct from per-package `nros.toml`): `[workspace]`
      (`default` deploy), `[system]` / `[systems.<name>]` (launch path,
      component refs, default `rmw` + `domain_id`, `[overlays.*]`), and
      `[deploy.<name>]` tables. Loader + schema + `nros check` validation.
      **Supersedes** the per-package "system `nros.toml` with
      `target.{triple,board}`" — triple/board move to `[deploy.<name>]`.
      Component `nros.toml` stays optional (reusable intrinsics only) and
      **must not** carry `rmw`/`domain`. Files: `nros-cli-core`
      orchestration loader, `cmd/check`, the schema types. Foundation for
      O–T.

- [ ] **172.P.M — Modularize `generate.rs` (Wave 1 enabler).** Mechanical
      refactor splitting the monolithic `orchestration/generate.rs` into
      `generate/{mod,entry_lib,config_lower,bridge,render}.rs` along the
      seams the parallel lanes need: entry-lib emission (172.P),
      config lowering (172.R), bridge/open_multi (172.S), and the shared
      render helpers. **No behavior change** — pure move + re-export,
      verified by the existing `orchestration_*` tests staying green. This
      is what lets P/R/S run concurrently without colliding in one file;
      do it first, fast, by one hand.

- [ ] **172.P — Two-form entry lib (neutral wiring + granular C ABI).**
      *(Sub-items per the lane table: P.0 freeze ABI · P.1 compiled form ·
      P.2 source form · P.3 delete `render_main`.)*
      Make the generator emit the wiring as a **library**, not a
      per-platform `main`: a granular C ABI (`nros_<sys>_register_all`,
      per-node `register_<node>`, `build_executor`, `Config`) in one of
      two forms — **compiled** (`lib<sys>.a` + cbindgen `<sys>.h`) or
      **source** (generated crate + a vendor-includable CMake fragment via
      `add_subdirectory` + corrosion). `[deploy].emit` (or kind) selects;
      vendor-owns-toolchain → source. **DELETE** the `EntryKind` /
      `render_main` per-platform `main` emitter and the
      `HostedMain`/`BoardRun`/`ZephyrStaticlib` branching (`generate.rs`) —
      there is no generated `main` anymore; platform startup lives on the
      deploy side (self's startup is a thin generated `deploy/<default>/`
      shim calling the C ABI). Re-express the Zephyr staticlib path as the
      unified source-form lib. Files: `orchestration/generate.rs` (remove
      `render_main`), cbindgen header emit, the C-ABI surface.

- [ ] **172.Q — `nros deploy` command-runner + `deploy/<name>/`.**
      *(Sub-items: Q.0 var-set + step-model contract (Wave 1) · Q.1 runner ·
      Q.2 name resolution + strip the old build flags. Q.1 devs against a
      stub entry-lib emit until 172.P lands.)*
      New `nros deploy <name>` (and `nros build <name>` as the
      self/vendor-lib alias): assert `vendor.pin` → emit the entry-lib form → run
      `[deploy.<name>].build[]` → `package[]`, substituting `{self}`
      (= `deploy/<name>/`), `{entry_lib}`, `{entry_src}`, `{entry_header}`,
      `{board}`, `{target}`, `{vendor.dir}`. Three kinds — `self` /
      `vendor-lib` / `vendor-module` — with **no per-vendor code in
      nano-ros** (vendor steps are user-authored shell lines). `vendor.pin`
      drift assert + `nros doctor` check (closes the old G3); `package[]`
      owns post-build artifacting (mkifs / sign / `.px4`; old G4).
      **DELETE** the flag-driven system-build interface in `cmd/build.rs`
      (`--launch` one-shot, `--system-plan`, `--system-output`,
      `--system-package`, `--nano-ros-workspace`, `--target`/`--rmw`
      ad-hoc) — `nros build|deploy <name>` reading the root `nros.toml` is
      the only system path; the plain project-flavor `nros build`
      (cargo/cmake/west autodetect for a single crate) stays. Files:
      `cmd/deploy.rs` (new), `cmd/build.rs` (strip system flags + name
      resolution), the `[deploy.<name>]` runner.

- [ ] **172.R — Config lowering across net-owner × host/embedded.**
      *(Sub-items: R.0 lowering-matrix spec (Wave 1) · R lowering impl in
      `generate/config_lower.rs`.)*
      Lower
      the plan's config (domain, locator, transport, params) at generation
      per `(net_owner × host/embedded)`: host → env at runtime; embedded +
      NanoRosOwned → baked board `Config` + shared `<sys>_config.h`;
      embedded + RtosOwned → domain/locator baked + NIC config as a vendor
      Kconfig/defconfig fragment in `{self}` (deploy merges via the
      vendor's include hook); vendor-link (IVC) → channel in the shared
      header; uORB → params baked. C-ABI entry takes an optional `Config`
      override (`cfg = NULL` ⇒ baked/env); precedence **param > env >
      baked**. Builds on 172.J (`PlanTransport`) + Phase 173.7
      (`nuttx-net.defconfig`) + the `NetStack` enum. `[deploy].config.{fragment,merge}`
      expresses the RtosOwned fragment + its merge command.

- [ ] **172.S — Bridge placement + multi-domain in one binary.** Establish
      **node = one session (rmw + domain); bridge = spans sessions, not a
      node.** OUT bridge stays runtime `nros-bridge.toml` (172.L,
      unchanged) as its own deployable. IN bridge = build-time `[[bridge]]`
      + `[[domain]]` groups in root `nros.toml` → planner/generator emit
      `Executor::open_multi([SessionSpec])` + per-node `create_node_on`
      session assignment (the K.5 by-id binding generalizes here).
      `build.rmw` becomes a *set* when an in-binary cross-RMW bridge is
      present (linkme multi-backend); `nros check` warns when the target
      can't link the set → recommend an OUT bridge. Files: planner
      (`[[bridge]]`/`[[domain]]` → plan), `generate.rs` (open_multi main),
      `cmd/check` (feasibility).

- [ ] **172.T — `nros new` scaffolders + eject gradient + implicit
      shortcut.** `nros new --deploy <name> --kind <k> --target <t>`
      scaffolds `deploy/<name>/` (kind-specific: self main is generated so
      none; vendor-lib startup stub + linker script; vendor-module CMake
      shell + Kconfig + module entry) **and** appends a `[deploy.<name>]`
      table to root `nros.toml`. Implicit path: bare `nros build` →
      `[workspace].default`; `nros build <name>` resolves a `[deploy.<name>]`
      profile or a `deploy/<name>/` dir (one namespace) — no long flags.
      `--from-launch`/`--from-profile` to materialize from existing config.
      Files: `cmd/new.rs`, the `deploy/<name>/` templates per kind.

- [ ] **172.V — Per-vendor deploy templates (PARALLEL MATRIX — one task
      per platform).** The biggest fan-out for many hands: each platform's
      `deploy/<vendor>/` template is an **independent** unit — the
      kind-specific shell (compiled `.a` link line / `add_subdirectory`
      source fragment / vendor-module manifest), the `[deploy.<name>]`
      `build[]`/`package[]` example lines, the config-fragment hook
      (172.R), and a **sim/HW validation** (CI surface). Author against the
      Wave-1 var-set contract (172.Q.0) — no need to wait on the runner.
      Each row below is a separately assignable ticket (172.V.<plat>):

  - [ ] **V.posix** — `self`, hosted native (the `[workspace].default`); a
        thin generated startup shim calling the C ABI. Reference template.
  - [ ] **V.bare-metal** — `self`, cortex-m / riscv32 QEMU (linker script +
        `board::run` startup). 
  - [ ] **V.freertos** — `self`/`vendor-lib` (FreeRTOS-Kernel link). 
  - [ ] **V.zephyr** — `vendor-module`, source form via
        `rust_cargo_application` + west; re-home `integrations/zephyr/`. 
  - [ ] **V.nuttx** — `vendor-module`, external-app Kconfig/Make.defs;
        re-home `integrations/nuttx/`; RtosOwned defconfig fragment (172.R). 
  - [ ] **V.threadx** — `self`/`vendor-lib` (ThreadX + NetX). 
  - [ ] **V.esp-idf** — `vendor-module`, ESP-IDF component; re-home
        `integrations/esp-idf/`. 
  - [ ] **V.px4** — `vendor-module`, `EXTERNAL_MODULES_LOCATION` + uORB;
        re-home `integrations/px4/`; SITL as the CI surface. 
  - [ ] **V.orin-spe** — `vendor-lib`, FSP link via `NV_SPE_FSP_DIR` +
        IVC startup + secure-boot `package[]`; POSIX sim as the CI surface. 
  - [ ] **V.qnx** — `vendor-lib`/`self`, `qcc` toolchain file + `mkifs`
        `package[]`; QEMU image as the CI surface (new platform port — may
        spike a `platform-qnx` first). 

  Templates land independently; 172.U wires them into the fixture sweep +
  deletes the old `integrations/` shells they replace.

- [ ] **172.U — Migrate to the model + delete the dead paths (the flip).**
      The cut-over that makes the revised model the *only* model. Re-home
      the `integrations/<rtos>/` shells (Phase 139) as the `deploy/<name>/`
      vendor-module templates 172.T scaffolds (they already are per-RTOS
      shells re-exporting the root CMake). Convert the orchestration
      fixtures (`orchestration_e2e`) + at least one self, one vendor-lib
      (Orin POSIX sim), and one vendor-module (Zephyr or PX4-SITL) system
      to a root `nros.toml` + `nros deploy <name>`, replacing the
      generated-`main` fixtures. **Delete the superseded code** once
      nothing references it: the `render_main`/`EntryKind` remnants, the
      old `cmd/build.rs` system flags + their tests, the per-package
      `system nros.toml` triple/board reader, and any
      `build_generated_package` `main`-mode paths. Update the book
      (`ros2-user-workflow`, `configuration`, CLI ref) to the root-config +
      `nros deploy` workflow. Acceptance: `grep` shows zero references to
      the deleted surfaces; `just ci` green; the three sample systems
      deploy from one root file.

  - Re-evaluated under the model: **172.K.7** (multi-homing
    `[[transport]].interfaces`) still applies — it is transport-schema
    work orthogonal to deployment, carries forward as-is. **172.E**
    (metadata-mode sandboxing) stays blocked-on-driver, independent of
    Group 5.

## Acceptance criteria

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
- **Group 5 (O–T)** builds on the now-complete planning half (Groups
  1–4): it reuses the plan IR and changes only how the generated wiring
  is *shipped*. It owns a new config scope (root `nros.toml`) and the
  `nros deploy` surface; it touches `generate.rs` (entry-lib forms,
  172.P) and the planner (in-binary bridges, 172.S), so coordinate those
  with any late Group 2/3 work. **Decided 2026-05-28:** ejected
  deployment code lives under a top-level **`deploy/<name>/`** (not
  `src/<name>/`) — it is deployment glue, not an application package, and
  keeping it out of `src/` stops colcon from treating vendor module
  shells as buildable workspace packages. **Open fork** (172.S): whether
  an in-workspace bridge always anchors its own deployable vs. living in
  a normal app system's root config — leaning "its own deployable" to
  keep the node = one-session invariant + RMW-set feasibility analysis
  clean.
