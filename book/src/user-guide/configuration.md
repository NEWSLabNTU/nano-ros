# Configuration Guide

nano-ros config is **language-agnostic** and has **one home per concern** — no
setting lives in two places, and nothing is merged across files. The authored
surfaces are a universal `system.toml`, `package.xml`, and launch XML — plus
Kconfig for the embedded build (Zephyr). `Cargo.toml` and `CMakeLists.txt`
carry language-toolchain facts and nothing else. This guide covers what each
file owns, the network identity an embedded image bakes at build time, and the
standalone `config.toml` for hand-written `no_std` apps.

> Retired surfaces you may find in old material: the `nros.toml`
> per-package/workspace file (rejected by the CLI), the old `config.toml`
> `[network]`/`[zenoh]`/`[scheduling]` schema, **every
> `[package.metadata.nros.{node,entry,component,deploy.*}]` table**, and the
> `package.xml` `<nano_ros deploy=… board=… rmw=…/>` tuple. The last two moved
> into `system.toml` under
> [RFC-0098](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0098-generated-leaf-build-config.md);
> readers refuse them rather than reading them. Design of record:
> [RFC-0004](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0004-configuration-and-transports.md).

## One home per concern

| File | Owns | Per |
|------|------|-----|
| **`system.toml`** | **Everything about the deployment** — `[system]` (RMW, domain, capability axes like `features = ["param_services"]` and the `[safety]` block, tiers), `[[component]]` (what runs, and its entities), and one `[image.<id>]` per target (`board`, `locator`, and the image's address on a network, `net = { eth0 = { network = "qemu", host = 10 } }`; a project's own networks are `[network.<name>]`). One schema for Rust, C and C++, and the same file whether the project is one package or a workspace. | package, or bringup pkg |
| `Cargo.toml` | Rust toolchain facts only: crate, language deps, the RMW **feature menu** (`rmw-zenoh`/`rmw-cyclonedds`/`rmw-xrce`) the generated entry selects from. No board, no target triple, no deploy block. | Rust project |
| `CMakeLists.txt` | C/C++ toolchain facts only: targets and language deps, via `find_package(nano_ros)` + `nano_ros_add_executable` / `nano_ros_auto_add_library` + `nros_components_register_node` (RFC-0057; `nano_ros_node_register` remains as the compat spelling) / `nano_ros_entry`. No platform, board or RMW `set()`. | C/C++ project |
| `build/<image>/nros-cargo.toml` | **GENERATED, never edited** — every cargo setting the board choice implies: `[build] target`, `[target.*]` runner and rustflags, the cross compiler, the derived pool sizes as `[env]`, the nano-ros profiles, and the `[patch.crates-io]` rows that resolve nano-ros's registry-style names into your checkout. Written by `nros sync`, passed to cargo with `--config`. | image |
| `package.xml` | ROS package identity + msg `<depend>`s (codegen input for `nros generate`) | all |
| **`config.toml`** (standalone) | **Hand-written `no_std` direct-mode apps only** — `[node]` / `[[transport]]` / `[node.rt]`, compile-baked via `Config::from_toml(include_str!(…))`. Apps that use `nros::main!()` / codegen do **not** have one — they use `[image.<id>]` above. | embedded single-node app (no codegen) |

> **Boundary rule.** If a knob changes *what is compiled/linked as a language
> choice* — a C++ standard, an RMW feature the entry may select — it lives in
> the build file. Everything else about the deployment (components, board,
> domain, RMW, and the physical link + router address a target boots with)
> lives in `system.toml`. Nothing a board implies is authored anywhere: it is
> derived from the board descriptor into the generated settings file.

### Config home by language × scale

Since [RFC-0098](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0098-generated-leaf-build-config.md)
this table has one answer in all four cells, which is the point of it:

| | Single-node | Workspace |
|---|---|---|
| **Rust** | `system.toml` beside `Cargo.toml` | `system.toml` in the bringup pkg |
| **C / C++** | `system.toml` beside `CMakeLists.txt` | `system.toml` in the bringup pkg |

A single-package leaf and a workspace bringup use the same schema, so the
resolver has one input shape and you have one thing to learn. Resolution is a
**fixed precedence ladder**, not a merge: explicit CLI/build flag (`--rmw` /
`-DNROS_*`) > `system.toml` (`[image.<id>]` > `[system]`) > the board
descriptor's `[knobs]` > built-in default. `nros config show` prints the
resolved effective config with per-value provenance; `nros check` flags values
still sourced from legacy files.

Full design rationale: [RFC-0004 (configuration & transports)](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0004-configuration-and-transports.md);
RMW backend selection & lowering is [RFC-0031](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0031-rmw-selection-and-lowering.md).
For the multi-RMW runtime topic-forwarding bridge (a separate file/feature), see
[`nros-bridge.toml`](../reference/nros-bridge-toml.md).

## Embedded image config (codegen apps — the common case)

An embedded app built through `nros::main!()` (Rust) or the CMake entry
codegen (C/C++) declares its board and its network + router config **per
image**, in `system.toml`. The toolchain bakes it at compile time — there is
no config file on the device and nothing is parsed at runtime. The file is the
same in both languages:

```toml
# system.toml (e.g. examples/mps2-an385-baremetal/rust/talker)
[system]
name      = "qemu_bsp_talker"
rmw       = "zenoh"
domain_id = 0

[[component]]
pkg      = "qemu_bsp_talker"
class    = "qemu_bsp_talker::Talker"
name     = "talker"
dispatch = "deferred"          # optional: callbacks hand off to a ring

[image.mps2-an385-baremetal]
board   = "qemu-mps2-an385"
locator = "tcp/10.0.2.2:10500"
net     = { eth0 = { network = "qemu", host = 10 } }
```

The `[image.<id>]` key is the image's name — what you pass to
`nros build <image>` — and `board` is the only thing that has to change to
retarget it. `nros::main!()` bakes the image's network fields into a
`DeployOverlay` that `BoardEntry::run_with_deploy` applies onto the board's
boot `Config`; the C/C++ entry codegen bakes the same values into its
generated glue. Nothing is written in `Cargo.toml`, in `package.xml` or on a
`cmake` command line.

### Networks and addresses

A network fact has three owners (RFC-0103 D10): the **board** states its
interfaces, the **project** states its networks, and the **image** states its
address on one. The image's half lives in a **board network file** the image
names with `os_config` (RFC-0109 D4 — on a board where nano-ros is the OS, the
board package owns the device, so the address is in the board's own file), one
table per interface:

```toml
[image.talker]
board = "qemu-mps2-an385"
os_config = ["board-net.toml"]
```

```toml
# board-net.toml, beside system.toml
[eth0]
network = "qemu"
host = 10                      # 10.0.2.10 on QEMU's network
```

The older inline spelling, `net = { eth0 = { … } }` in `[image.<id>]`, is still
read; stating one interface in both places is refused. Zephyr and NuttX boards
own their network in their own configuration, and a host-OS board (Linux) in
the operating system, so neither takes a board network file.

`qemu` is built in: QEMU's own user-mode (SLIRP) network, `10.0.2.0/24` with
gateway and router host `10.0.2.2`. Every `-nic user` makes a private copy of
it, so two images on `qemu` never meet. Any other network is the project's,
stated once and named by every image on it:

```toml
[network.lab]                  # e.g. a switch or a Linux bridge with boards on it
subnet  = "192.168.50.0/24"
gateway = "192.168.50.1"
router  = "tcp/192.168.50.1:7447"    # the locator its images dial by default

[image.sensor]
board = "qemu-mps2-an385"
os_config = ["board-net-sensor.toml"]   # [eth0] network = "lab", address = "192.168.50.20"
```

Per interface:

- `network` defaults to the board's (`qemu` on every QEMU board), so
  `[eth0] host = 10` is enough there.
- `host` is the last octet within the network's subnet; `address` is the full
  IPv4, or `"dhcp"` on a network with `dhcp = true` (and a board whose
  interface runs a DHCP client). An interface on `qemu` that states no address
  takes `10.0.2.15`, the address a plain `-nic user` guest picks.
- `mac` is optional. Left out, it is derived from the board interface's
  `mac_prefix` and the system, image, network and interface names, so two
  images never share one by accident.
- The gateway and netmask come from the network, so an image never states them.
- An image dials its own `locator`, else its network's `router`, else
  `[system] locator`. On a network other than `qemu` one of the three must be
  stated.

A board may have several interfaces. An image addresses each on its own
network, one table each: `[eth0]`, `[eth1]`.

These fail the build, naming the image (and, for a duplicate, both images):

- an address outside its subnet, equal to the gateway, or on a network that
  does not exist;
- two images with the same static address, or the same MAC, on one network
  other than `qemu`;
- `net` on a board whose network belongs to the host operating system (`linux`,
  `freertos-posix`, threadx-linux), or on a board that declares no interfaces
  (Zephyr boards take their address from Kconfig);
- the old flat `ip` / `gateway` / `netmask` keys on `[image.<id>]`.

The board's half is `[board.net]` in its descriptor: `network` (its default
network) and `interfaces`, each a kind (`eth0 = "ethernet"`) or a table
(`eth0 = { kind = "ethernet", mac_prefix = "52:54:00", dhcp = true }`). A board
states no address: `[board.net] ip` / `mac` are refused.

The resolved answer for an image is written to
`build/nros/net/<image>.toml` (beside the workspace or leaf) and reaches the firmware as
`NROS_IMAGE_NET`.

An emulated image joins its network through the network's `attach`:
`"slirp"` (QEMU's own user-mode network, what `qemu` is), `{ bridge = "<dev>" }`
or `{ tap = "<dev>" }` on the host. A network with no `attach` is physical only,
and an emulated image on it is refused at launch. `nros ws qemu-nic <leaf>
--model <nic-model>` prints the `-nic` value for a leaf's image, for a script
that starts QEMU itself:

```toml
[network.br]
subnet = "192.0.3.0/24"
gateway = "192.0.3.1"
router = "tcp/192.0.3.1:7447"
attach = { bridge = "qemu-br" }   # QEMU: -nic bridge,br=qemu-br,…
```

### Per-image build knobs

A build knob that one image needs, and that its board does not state, goes in
`[image.<id>.knobs.<group>]`. The table has the same shape as a board
descriptor's `[knobs.<group>]`:

```toml
[image.zephyr_xrce.knobs.zephyr]
heap_size = 98304            # NROS_ZEPHYR_HEAP_SIZE

[image.zephyr_xrce.knobs.xrce]
max_subscribers = 1          # NROS_XRCE_MAX_SUBSCRIBERS (-1, the default, derives it)

[image.linux.knobs.log]
max_level = "warn"           # NROS_LOG_MAX_LEVEL (a level name or 0..6)

[image.linux.knobs.executor]
max_cbs = 16                 # NROS_EXECUTOR_MAX_CBS
```

Each key is a row of the knob registry
(`packages/tooling/nros-zephyr-build/knobs.toml`). The key is the row's board
key where it has one (`executor.max_cbs`, `zenoh.tx.batch`); otherwise it is
the env name without `NROS_`, split at the first `_` (`NROS_ZEPHYR_HEAP_SIZE`
becomes `zephyr.heap_size`). An unknown key, a value of the wrong type or a
value outside the knob's range stops `nros sync` / `nros build` with an error
that names the key. The value reaches the build exactly as the knob's
`NROS_*` variable would: through the generated cargo config on the cargo
road, the image env file on the cmake road, and the Kconfig defaults on west.
It sits on the image rung (above the board, below an exported variable).

`[image.<id>] env = { NAME = "value" }` still carries an application's own
build variable, a name the registry does not hold. An `env` row that names a
registry knob is an error that names its `knobs` spelling (release 0.6.0
accepted it with a warning).

RT/stack/priority for a single-node embedded app comes from **board-crate
Cargo features + Kconfig** (`prj*.conf` on Zephyr), not a config file.
Multi-node RT is declared in `system.toml` (`[tiers.<name>.<rtos>]` +
`[[node_overrides]]`) — see RFC-0015.

### Periodic timers after a stall

If a tier is preempted past several periods of a periodic timer, the backlog is
**coalesced, not replayed**: the timer fires ONCE and the missed periods are
counted. Phase is preserved, so activations stay on the declared cadence grid
rather than drifting by each stall. This is the default (`TimerOverrunPolicy::Skip`)
and matches rclcpp and Zephyr's `k_timer`.

Replay is available — `Executor::set_timer_overrun_policy(.., CatchUp)` — and is
the right choice for a counter or accumulator, where every tick carries state
that must not be lost. It is the wrong choice for a control loop: the replayed
activations compute with stale inputs and emit a burst of commands inside a
fraction of a period.

Overruns are counted per timer (`Executor::timer_overruns`) and reported as the
`timer-overrun-runtime` violation. Watch that rather than a publish-rate check:
under `CatchUp` a replayed burst makes the achieved rate look nominal — measured
100.03 Hz on a declared 100 Hz loop while the tier was stalling for up to 611 ms
at a time — so a rate monitor reports health during exactly the fault it exists
to catch. Rationale and the measurements: RFC-0002 § 4.4a.

Timer periods are microseconds throughout, including across the C ABI.

## Standalone `config.toml` — hand-written `no_std` apps (no codegen)

A hand-written single-node app that writes its own `main()` and bypasses
`nros::main!()` keeps its net config **in a file, not hardcoded in Rust**: a
sibling `config.toml` carrying `[node]` / `[[transport]]` / `[node.rt]`,
compile-baked with `include_str!` and parsed by the board crate's
`Config::from_toml`.

```toml
# config.toml

[node]
domain_id = 0              # ROS 2 domain ID (0–232)
# namespace = "/"
# rmw = "zenoh"            # ACTIVE backend; must match a LINKED backend
                           # (the build file picks what is linked)

# One transport per session. A single ethernet/wifi/serial entry is the
# common case. In-process topic forwarding is the separate [[bridge]] path.
[[transport]]
kind    = "ethernet"       # ethernet | wifi | serial | can
ip      = "10.0.2.10/24"   # CIDR — the prefix rides the address
mac     = "02:00:00:00:00:00"
gateway = "10.0.2.2"
locator = "tcp/10.0.2.2:7447"

[node.rt]                  # scheduling / real-time (RTOS); omit for defaults
app_priority         = 12
app_stack_bytes      = 65536
zenoh_read_priority  = 16
zenoh_lease_priority = 16
poll_priority        = 16
poll_interval_ms     = 5
```

Per-kind transport fields:

| kind | fields |
|------|--------|
| `ethernet` | `ip` (CIDR), `mac`, `gateway`, `interface` |
| `wifi` | `ssid`, `password`, optional static `ip`/`gateway` |
| `serial` / `can` | `device`, `baudrate` |
| all | `id`, `rmw`, `locator` |

`ip` is CIDR (`10.0.2.10/24`) — the board derives the prefix or netmask from it.
A serial locator carrying `#` (`serial/UART_0#baudrate=115200`) is fine — quote it.

Consumption (compile-baked; no filesystem on the target):

```rust
use nros_board_mps2_an385::{Config, run};

fn main() -> ! {
    run(Config::from_toml(include_str!("../config.toml")), |config| {
        let exec = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id);
        // ...
    })
}
```

Pick by whether the app uses codegen: `nros::main!()` app → `system.toml`
`[image.<id>]` (above); hand-written `main()` → `config.toml`. Only the **old**
`config.toml` schema (`[network]`/`[zenoh]`/`[scheduling]`) is retired.

## Retired files

- **`nros.toml` — retired in full.** A workspace-root `nros.toml`
  is rejected by the CLI (`NrosTomlNotSupported`; migrate with the
  `nros-v0.5.0` tag's `nros migrate workspace` — the one-shot verb is retired
  on newer trees, #186); the legacy per-package overlay is a deprecated fallback that
  `nros check` flags; the embedded-runtime role never shipped (no example
  ever declared one). If a doc tells you to write `nros.toml`, it predates
  the migration — the content belongs in `system.toml`.
- **Old `config.toml` schema** (`[network]`/`[zenoh]`/`[scheduling]`) —
  retired; superseded by `[image.<id>]` (net) + board
  features / Kconfig (RT). The direct-mode `config.toml` above is the kept,
  supported shape.

## Build-time environment variables

Read during `cargo build` (by `build.rs`) or by justfile recipes. Set in `.env`
or export in your shell.

### SDK paths

You normally set none of these. `nros setup` puts each vendored source tree
in the shared store, and every build finds it through one ladder (RFC-0103
D3): an environment override, then your project's `[sources]` in
`system.toml`, then a checkout submodule you are editing, then the store,
then the checkout's own copy. Ask where a tree came from with
`nros locate <name> --why`.

Set one of these variables only to build against a tree of your own:

| Variable | Tree |
|----------|------|
| `FREERTOS_DIR` / `LWIP_DIR` | FreeRTOS kernel / lwIP |
| `THREADX_DIR` / `NETX_DIR` | ThreadX / NetX Duo |
| `NUTTX_DIR` / `NUTTX_APPS_DIR` | NuttX RTOS / apps |
| `CYCLONEDDS_SOURCE_DIR` | Cyclone DDS source |

For a tree that belongs to one project, prefer `[sources] <name> = "<path>"`
in its `system.toml` (project-relative) over an exported variable, which
every build in that shell inherits.

`FREERTOS_PORT`, `FREERTOS_CONFIG_DIR`, `THREADX_CONFIG_DIR` and
`NETX_CONFIG_DIR` are not yours to set: they are facts about the board, stated
in its `nros-board.toml` and carried to the build from there.

Buffer-tuning vars (`ZPICO_*`, `XRCE_*`, `NROS_*`) are optional — see the
[Environment Variables Reference](../reference/environment-variables.md).

### Binary-size knobs (embedded)

On a constrained MCU, two build-time env vars — carried in the image's
generated `build/<image>/nros-cargo.toml` `[env]`, like the other `NROS_*`
tuning, and set there from the board descriptor's `[knobs]` or from your own
environment — shed the parts a brokered client doesn't need:

| Variable | Default | Effect |
|----------|---------|--------|
| `NROS_LINK_IP` | on | `NROS_LINK_IP=0` on a **serial-only** node drops the IP link layer — zenoh-pico's TCP/UDP link C (`Z_FEATURE_LINK_TCP/UDP=0`) and (with `--gc-sections`) the smoltcp platform impl. Serial link stays. |
| `NROS_SMOLTCP_MAX_SOCKETS` / `NROS_SMOLTCP_MAX_UDP_SOCKETS` | 4 / 2 | Sized for DDS RTPS (3 UDP/participant). A zenoh/XRCE client multiplexes everything over **one** session → set both to `1` to drop the spare socket buffers (≈8 KB each). |
| `NROS_HEAP_SIZE` | per-platform-crate (64 KB `nros-platform-mps2-an385`, 32 KB `nros-platform-stm32f4`) | Decimal **bytes** for the bare-metal static heap. The defaults are generous; size to the RMW's working set (table below). E.g. `NROS_HEAP_SIZE = "24576"` on a zenoh-pico node cut the mps2-an385 `.data` 66 → 25 KB (−41 KB). |

**Static-heap sizing by backend** (bare-metal `FreeListHeap`, set via
`NROS_HEAP_SIZE`):

| Backend | Peak working set | Recommended heap | Notes |
|---------|------------------|------------------|-------|
| zenoh-pico (TCP) | ~16 KB | **24–32 KB** (≈2× for fragmentation) | peer middleware; `alloc`-based session/buffers |
| zenoh-pico (serial) | lighter than TCP | **16–24 KB** | no TCP link buffers; verified running at 16 KB |
| XRCE (Micro-XRCE-DDS) | ~3 KB (micro-ROS figure) | **~8 KB** | static pools, discovery offloaded to the agent — the RAM-minimal backend; a measured bare-metal XRCE figure is pending an example (no bare-metal XRCE example ships yet — XRCE bare-metal needs a custom-transport injection) |

### Measured footprint

Honest, reproducible numbers per `(platform, transport, backend, profile)` —
built with the in-tree examples, the **release** profile is cargo's default
(opt-3), the **size** profile is the scaffolded `[profile.size]` (opt-`s` + `lto`
+ `strip`, see the [size-tuned release profile notes](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/roadmap/phase-204-embedded-binary-size.md#2043--size-tuned-embedded-release-profile)).
RAM = `data + bss`. All cells are after `--gc-sections` + the size knobs above
are applied where noted; the serial cell ships with the recipe below.

| platform | transport | backend | profile | text (flash code) | data | bss | RAM total |
|---|---|---|---|---|---|---|---|
| mps2-an385-baremetal (cortex-m3) | ethernet (smoltcp) | zenoh-pico | release | 177.4 KB | 67.0 KB | 91.7 KB | **158.7 KB** |
| mps2-an385-baremetal | ethernet | zenoh-pico | size | **158.3 KB** | 67.0 KB | 91.7 KB | 158.7 KB |
| mps2-an385-baremetal | **serial** (no IP stack) | zenoh-pico | release | 128.6 KB | 25.2 KB | 75.8 KB | **101.0 KB** |
| mps2-an385-baremetal | **serial** | zenoh-pico | **size** + recipe | **116.1 KB** | 25.2 KB | 75.8 KB | **101.0 KB** |
| stm32f4 (thumbv7em-eabihf, cortex-m4) ¹ | ethernet | zenoh-pico | release | 186.9 KB | 13.7 KB | 123.0 KB | 136.7 KB |
| stm32f4 ¹ | ethernet | zenoh-pico | size | **138.1 KB** | 13.7 KB | 123.0 KB | 136.7 KB |
| mps2-an385-freertos (cortex-m3 + lwIP, RTOS-reused stack) | ethernet (lwIP) | zenoh-pico | release | 240.6 KB | 10.7 KB | 3.3 MB | 3.3 MB |
| **mps2-an385-baremetal (serial)** | **serial** (custom XRCE transport) | **XRCE** | **size**, heap 24 KB, tight XRCE pools | **60.3 KB** | 25.2 KB (heap 24 KB) | 8.8 KB | **~34 KB** |
| **micro-ROS reference** (XRCE) | serial | XRCE-DDS Client | -Os | < 75 KB | — | ~3 KB | ~3 KB peak |

¹ The two stm32f4 rows are **historical**: the board crates left the tree in
a later cleanup, so nothing in-tree reproduces them today. They stay because a
Cortex-M4F-with-its-own-stack data point is still the closest published figure
for that class, and deleting a measurement is worse than dating it. The chip
crate the numbers were built against (`nros-platform-stm32f4`) is still here.

The XRCE row uses tight per-session XRCE pools — delivered through the
image's generated `[env]` and read by `nros-rmw-xrce-cffi`'s
`build.rs`: `NROS_XRCE_STREAM_HISTORY=4`,
`NROS_XRCE_CUSTOM_TRANSPORT_MTU=512`, `NROS_XRCE_MAX_SUBSCRIBERS=1`,
`NROS_XRCE_MAX_SERVICE_SERVERS=1`, `NROS_XRCE_MAX_SERVICE_CLIENTS=1`,
`NROS_XRCE_SUBSCRIBER_RING_DEPTH=1`, `NROS_XRCE_BUFFER_SIZE=256`. Vendor
defaults grow `xrce_session_state_t` to ~390 KB (which wouldn't fit a
24 KB heap); these knobs drop it to ~12 KB. Defaults are unchanged for
hosted / non-tight-RAM consumers — the env vars are pure opt-in.

**How to read this:**

- **The size profile (opt-`s`) shrinks `.text` by ~10–26 %** with `.bss`/`.data`
  unchanged (opt-level doesn't touch static buffers — those are the env knobs
  above). `-Oz` is **not** used — on smoltcp examples it grows `.bss` +24 KB by
  defeating opt-3's per-socket dead-buffer DCE (see the size-tuning notes above).
- **Switching ethernet → serial sheds ~50 KB text + ~42 KB `.data`** (no smoltcp
  stack, no IP link C, tuned heap) — the structural lever.
- **FreeRTOS + lwIP cells `.bss` is dominated by lwIP's heap + FreeRTOS task
  stacks** (3 MB is the configured headroom, not nano-ros overhead).
- **The micro-ROS / XRCE row is a reference**, not a nano-ros measurement — no
  bare-metal XRCE example ships yet (needs a custom-transport injection); the
  path to parity is **XRCE + serial + static pools**.

### Size-minimal recipe

Smallest measured nano-ros configuration today (mps2-an385-baremetal serial
talker, **116 KB text / 101 KB RAM**):

```toml
# Cargo.toml
[profile.size]
inherits = "release"
opt-level = "s"
lto = "fat"
codegen-units = 1
debug = false
strip = true
```

The gc-sections and link-script rustflags are **not yours to write** — they are
the board's, and they arrive in the generated
`build/<image>/nros-cargo.toml` `[target.<triple>]` block:

```toml
# build/<image>/nros-cargo.toml — GENERATED, shown for orientation
[target.thumbv7m-none-eabi]
rustflags = [
    "-C", "link-arg=-Tlink.x",
    "-C", "link-arg=--gc-sections",   # 204.8 — strip unreferenced fns/data
]
```

The knobs that *are* yours are ordinary environment variables. The generated
`[env]` never uses `force`, so a value you set in the calling environment wins:

```bash
NROS_LINK_IP=0 \
ZPICO_NO_SMOLTCP=1 \
NROS_HEAP_SIZE=131072 \
NROS_SMOLTCP_MAX_SOCKETS=1 \
NROS_SMOLTCP_MAX_UDP_SOCKETS=1 \
    nros build <image>
```

The profile is not one of them: name it on the image, as
`[image.<id>] profile = "size"`, and `nros build` passes it to whichever tool
it drives.

`NROS_LINK_IP=0` drops zenoh-pico's TCP/UDP link C (204.7) and
`ZPICO_NO_SMOLTCP=1` skips the smoltcp glue on bare-metal. On the heap floor:
the per-entry executor backing is a single ~75 KB allocation, so a
`nros::main!` image needs ≥128 KB (the #176 board default) — the pre-271 24 KB
"zenoh-pico working set" figure OOMs at boot. `HEAP` is `.bss` (no flash cost);
shrink below the default only on a non-`nros::main!` direct-mode image with a
measured smaller peak. A knob you always want for a board belongs in that
board's descriptor `[knobs]`, not on every command line.

`nros new --platform baremetal` already scaffolds the `[profile.size]` shape
(Phase 204.7/204.8).

**The deeper RAM win waits on XRCE on bare-metal** (the ~3 KB-class client +
static pools, with discovery offloaded to the agent) — tracked separately;
zenoh-pico's `SUBSCRIBER_BUFFERS` + alloc-based session are what keep this row's
`.bss` ~76 KB.

## Cargo features (which RMW/platform is *linked*)

Features select the **linked** RMW backend, platform, and ROS edition. The
`system.toml` `rmw` (`[image.<id>]`, else `[system]`) picks which *linked*
backend is *active* — the two are different layers (link vs run). Matrix +
mutual-exclusion rules: [Platform Model](../concepts/platform-model.md).

```toml
[dependencies]
nros = { path = "…/nros", default-features = false, features = [
    "rmw-cffi",            # generic C-vtable runtime registry
    "platform-bare-metal", # or platform-{freertos,nuttx,threadx,zephyr,posix}
    "ros-humble",          # or ros-iron
    "std", "alloc",        # optional, target-dependent
] }
# Exactly one RMW backend crate; its registration runs before main:
nros-rmw-zenoh = { path = "…/nros-rmw-zenoh", features = ["platform-bare-metal", "link-tcp", "ros-humble"] }
# …or nros-rmw-xrce-cffi / the cyclonedds CMake backend
```

## Runtime environment (native host only)

On the `native` host build, `ExecutorConfig::from_env()` reads at process start
(embedded targets bake their config at build time instead):

| Variable | Description | Default |
|----------|-------------|---------|
| `ROS_DOMAIN_ID` | ROS 2 domain ID | `0` |
| `NROS_LOCATOR` | Router address | `tcp/127.0.0.1:7447` |
| `NROS_SESSION_MODE` | `client` / `peer` | `client` |
| `ZENOH_TLS_ROOT_CA_CERTIFICATE*` | TLS CA cert (path / base64) | (none) |

## By deployment scenario

| Scenario | Config source | Cargo features | Notes |
|----------|---------------|----------------|-------|
| Desktop (POSIX) | env (`ExecutorConfig::from_env()`) | `rmw-cffi, platform-posix, std` + zenoh dep | run the router locally (`ros2 run rmw_zenoh_cpp rmw_zenohd`, `ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false'`) |
| QEMU bare-metal | `system.toml` `[image.<id>]` ip/mac/gateway/locator | `rmw-cffi, platform-bare-metal, ros-humble` + zenoh | TAP/slirp bridge |
| FreeRTOS hardware | `[image.<id>]` + board features (RT) | `…, platform-freertos, …` | `FREERTOS_DIR`/`LWIP_DIR` |
| ESP32 WiFi | `[image.<id>]` (`ssid`/`password` via build env) | `…, platform-bare-metal, …` | `SSID`/`PASSWORD` build env |
| Zephyr module | Kconfig overlay (`prj-<rmw>.conf`) | (Kconfig → features) | |
| Hand-written `no_std` (no codegen) | standalone `config.toml` → `Config::from_toml` | per-board | net config in a file, not in code |
| Minimal RAM (XRCE serial) | `config.toml [[transport]] kind="serial"` or `[image.<id>]` | `…` + xrce dep | `XRCE_*` buffer tuning |

## `.env`

```bash
cp .env.example .env   # uncomment + adjust; gitignored; auto-loaded by just + direnv
```
