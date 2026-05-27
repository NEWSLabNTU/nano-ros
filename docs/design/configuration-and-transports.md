# Configuration model: `nros.toml`, transports, and node binding

**Status:** Approved design (Phase 172.K, 2026-05-27). Supersedes the per-example
`config.toml` (`[network]`/`[zenoh]`/`[scheduling]`) and the never-shipped
`nano-ros.toml` idea (archived Phase 116).

## One file, two read modes

`nros.toml` is the single, language-agnostic nano-ros project config. It is read
two ways from the **same** schema:

- **Direct mode** — a hand-written single-node app reads its `nros.toml` via the
  board `Config::from_toml` (compile-baked with `include_str!` on embedded;
  filesystem/env on hosted). No launch file, no planner, no generated `main`.
  This is what the `examples/**` copy-out templates use ("boilerplate IS
  lesson" — they keep their hand-written `main()`).
- **Planned mode** — the orchestration pipeline (Phase 126): launch files +
  component metadata + the system `nros.toml` → `nros plan` → `nros-plan.json` →
  generated `main()` → one binary with all nodes wired at compile time.

`package.xml` owns ROS identity + msg `<depend>` (codegen) in both modes;
`.cargo/config.toml` is dep-injection only; `Cargo.toml`/`CMakeLists.txt` own the
build. `nros.toml` owns all nano-ros runtime/deployment config.

## Transports are top-level, decoupled, `id`-addressable

A **transport** is a physical link + the RMW session that rides it. Transports are
declared at top level, independent of nodes:

```toml
[[transport]]
id      = "eth"            # optional; defaults to `rmw` when each rmw is unique
kind    = "ethernet"       # ethernet | wifi | serial | can
ip      = "10.0.2.50/24"   # ethernet/wifi; CIDR carries the prefix
mac     = "02:00:00:00:00:01"
gateway = "10.0.2.2"
rmw     = "zenoh"          # the RMW session this transport runs
locator = "tcp/10.0.2.2:7447"

[[transport]]
id       = "bus"
kind     = "serial"
device   = "UART0"
baudrate = 115200
rmw      = "cyclonedds"

# wifi adds credentials:
# [[transport]] kind = "wifi"  ssid = "Net"  password = "secret"  rmw = "zenoh"
```

Per-kind field rules (validated by `PlanBuildOptions::validate_transports`):

| kind | fields |
|------|--------|
| `ethernet` | `ip` (CIDR), `mac`, `gateway` |
| `wifi` | `ip` (optional/static), `ssid`, `password` |
| `serial` / `can` | `device`, `baudrate` |
| all | `id`, `rmw`, `locator` |

The `id` makes a transport first-class and addressable, and disambiguates two
transports that share an `rmw` (which a bind-by-`rmw` scheme cannot).

## Runtime mapping

Each `[[transport]]` becomes one `SessionSpec { rmw, locator, domain_id, … }`.
The executor opens them with `Executor::open_multi([specs])`; a node's entities
ride the session of the transport it is bound to (`create_node_on`).

## Binding: how nodes connect to transports

A node binds to exactly one transport (= one session). Binding is keyed by
transport `id`, with implicit defaulting for the common cases:

| transports | binding | executor |
|------------|---------|----------|
| **0** | board-default link + the single linked RMW; zero-config | `Executor::open` |
| **1** | every node rides it implicitly; no binding syntax | `Executor::open` |
| **N** | each node names its transport; unbound nodes fall to the `default = true` transport | `Executor::open_multi` |

**Where the link is declared:**

- **Direct mode** — one implicit node rides the single/default transport; no
  binding syntax. (Direct mode is single-node by definition; the N-transport
  case is the planned/bridge path.)
- **Planned mode** — nodes come from launch; the system `nros.toml` binds
  *instances* to transports:

  ```toml
  [[component]]
  package = "sensor_pkg"; component = "sensor"
  transport = "eth"        # this component's nodes ride the eth/zenoh session

  [[component]]
  package = "logger_pkg"; component = "logger"
  transport = "bus"        # omitted ⇒ the default transport
  ```

**Runtime note.** `create_node_on(name, rmw)` selects a session by RMW name
today. When every transport has a distinct `rmw`, `id == rmw` and binding works
unchanged. Binding two transports that share an `rmw` (distinct `id`s) needs
`create_node_on` to select by transport id / session index — a small, additive
runtime extension; until it lands, the bind-by-`rmw` path covers the common
multi-RMW bridge.

## Scheduling / RT — `[node.rt]`

Scheduling has no transport home; it stays a node-level block (replacing
`config.toml [scheduling]`), and in planned mode maps to the `SchedContextConfig`
the planner already carries:

```toml
[node.rt]
app_priority = 12;  app_stack_bytes = 262144
zenoh_read_priority = 16;  zenoh_read_stack_bytes = 5120
zenoh_lease_priority = 16; zenoh_lease_stack_bytes = 5120
poll_priority = 16; poll_interval_ms = 5
```

## Worked examples

### Single-node ethernet (the common direct-mode case)

```toml
[node]
domain_id = 0

[[transport]]
kind    = "ethernet"
ip      = "10.0.2.10/24"
mac     = "02:00:00:00:00:00"
gateway = "10.0.2.2"
rmw     = "zenoh"
locator = "tcp/10.0.2.2:7450"
```

### ESP32 wifi

```toml
[node]
domain_id = 0

[[transport]]
kind     = "wifi"
ssid     = "MyNetwork"
password = "secret"
rmw      = "zenoh"
locator  = "tcp/192.168.1.1:7447"
```

### Two-transport bridge (planned mode)

```toml
[node]
domain_id = 0

[[transport]]
id = "eth"; kind = "ethernet"; ip = "10.0.2.50/24"; rmw = "zenoh"; locator = "tcp/10.0.2.2:7447"
[[transport]]
id = "bus"; kind = "serial"; device = "UART0"; baudrate = 115200; rmw = "cyclonedds"

# instances bind via [[component]] transport = "eth" / "bus" (see above)
```

## Migration from `config.toml` (Phase 172.K)

| `config.toml` | `nros.toml` |
|---------------|-------------|
| `[zenoh] locator`/`domain_id` | `[node] domain_id` + `[[transport]] locator` |
| `[network] ip`/`mac`/`gateway`/`prefix`/`netmask` | `[[transport]] kind="ethernet"` `ip` (CIDR) / `mac` / `gateway` |
| `[wifi] ssid`/`password` | `[[transport]] kind="wifi"` `ssid` / `password` |
| `[serial] baudrate` | `[[transport]] kind="serial"` `device` / `baudrate` |
| `[scheduling] *` | `[node.rt] *` |
| `[platform] interface` (threadx-linux) | `[[transport]] interface` |

The 8 board `Config::from_toml` parsers read the new shape; the 88 example
`config.toml` files + `include_str!` sites move to `nros.toml`; `config.toml` is
deleted.

## See also

- Phase 172 (`docs/roadmap/phase-172-orchestration-deferred.md`) — work items
  J–N; the configuration consolidation absorbed from Phase 116.
- Phase 126 (archived) — the orchestration pipeline (planned mode).
- `book/src/reference/nros-bridge-toml.md` — the separate runtime topic-forward
  bridge config (do not confuse with this build/deploy `nros.toml`).
