---
rfc: 0140
title: "ROS 2 environment variables and what system.toml states"
status: Draft
area: workspace-system-model
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-581]
amends: []
supersedes: []
superseded-by: null
---

# RFC-0140 — ROS 2 environment variables and system.toml

**Status:** Draft (2026-11-19). Nothing is built. It generalises one user
ruling, made for `ROS_DOMAIN_ID` and implemented for the domain and the
locator by issue 2714 (PR #4285). Every other
row below is a proposal; the user's 2026-11-19 ruling on the open questions
is recorded under "Decisions" at the end, and reopens Q1 with a wider
scope. Tracked by
[phase-581](../roadmap/phase-581-ros-env-vars.md).

## Summary

A ROS 2 user exports environment variables (`ROS_DOMAIN_ID`,
`RMW_IMPLEMENTATION`, `ROS_LOCALHOST_ONLY`, `CYCLONEDDS_URI`, ...) and
expects a node to act on them. A nano-ros image also has a `system.toml`
that states many of the same facts. This RFC decides, per variable, which one
wins, and how a user learns about it.

The rule, from the user's ruling on `ROS_DOMAIN_ID`:

1. **The value `system.toml` states is respected.** It is the image's value
   when nothing overrides it.
2. **On a Linux process, an environment value overrides it.**
3. **A mismatch is warned, never silent.** When both state a value and they
   differ, the image says so at boot, once.

Every variable gets one of three dispositions:

- **RESPECT**: the rule above.
- **IGNORE**: nano-ros does not act on it. The docs say so, and a process
  that finds it set prints a one-time notice naming what to use instead.
- **REFUSE**: a set value stops the boot with a reason (the two retired
  `ZENOH_*` aliases today; a security request nano-ros cannot honour).

The decision lives in ONE place for C, C++ and Rust: a registry of every
environment variable a nano-ros process reads, beside the build-time knob
registry, and one small reader below every backend. A gate refuses a new
environment read that does not go through it.

## Motivation

### The worked example: issue 2714

Issue 2714 found that a `linux` image dropped the `domain_id` and `locator`
its `system.toml` states, on every language road. The Rust board's baked rung
left them out on purpose (issue #48's reasoning: "keep the environment
authoritative"), and the C/C++ host runners passed NULL and 0. A deployment
that stated domain 7 got domain 0, with nothing said.

The fix (PR #4285) puts the rule in three places:

| Piece | Where |
| --- | --- |
| The precedence (env > baked > default) | `nros_node::ExecutorConfig::try_resolve_with`, unchanged; reached through `nros::env::try_resolve_hosted` |
| The pure mismatch decision | `nros::env::env_overrides_statement`, returning `StatementOverrides { domain_id, locator }` |
| The warning | `nros::env::warn_env_overrides_statement`, through `nros_log::log_warn!` on logger `nros`: `ROS_DOMAIN_ID=7 overrides system.toml's domain_id=42` |
| Rust caller | `nros-board-linux::resolve_hosted_boot` (both funnels) |
| C/C++ caller | `nros-cpp`'s `warn_env_overrides_stated`, called by each `nros_board_native_run_*_in` runner, and exported as `nros_cpp_warn_env_overrides_stated` for `rclcpp::init_in` |

So the decision has one implementation, in Rust, and both language roads
reach it. That is the right shape, and this RFC keeps it.

**Can `nros::env` be THE one implementation point for every variable? Not
where it sits.** Measured (below), nano-ros reads environment variables in at
least eleven runtime files, and four of them cannot reach `nros::env`:

- `nros-rmw-zenoh` (`src/shim/session.rs`) is below the `nros` crate in the
  dependency graph; it reads `ZENOH_SESSION_CONFIG_URI` and seven `ZENOH_*`
  names itself.
- The Cyclone backend is C++ (`nros-rmw-cyclonedds/src/env_compat.hpp`,
  `service.cpp`, `graph.cpp`); it can call a C symbol, not a Rust module.
- `nros-rmw-cffi` (`src/lib.rs`, `NROS_RMW_TRACE_OPEN`) and the XRCE C
  transport (`transport_posix_serial.c`, `XRCE_SERIAL_BAUD`) are below it too.
- Cyclone DDS's own config loader reads `CYCLONEDDS_URI` inside third-party
  code, which nano-ros does not edit.

Also, `nros::env` is compiled only for `target_os = "linux"`
(`packages/api/nros/src/lib.rs`, `#[cfg(target_os = "linux")] pub mod env`),
and its module comment claims "Every env var nano-ros honours is read HERE
and nowhere else", which the inventory shows is false. D4 moves the reader
and the decision one level down, so every reader can reach them, and keeps
`nros::env` as the boot-config ladder that calls them.

### The three questions a user asks

- "I exported X. Did my node use it?" Today the answer is in the code. Only
  the boot line (`nros::env::report`: `boot config: domain N (env), locator
  ... (baked), rmw ... (default)`) says where three fields came from.
- "My `system.toml` says Y. Why does the node use something else?" Before
  issue 2714, nothing said. After it, two fields warn.
- "Does this variable do anything on my board?" On an embedded target, never;
  nothing says so.

## Inventory (measured 2026-11-19)

**Method.** In this tree, `git grep` for each variable name over
`packages/{api,core,rmw,platform,boards,drivers}`, `cmake/` and `zephyr/`,
excluding `tests/`, `third-party/`, generated code and Markdown; then every
runtime read primitive (`getenv(`, `std::getenv`, `env::var(`,
`env::var_os(`, `nros::env::var(`, `env_lookup(`, `option_env!(`), with each
hit read to classify it. Build scripts (`build.rs`, `*_build.rs`) are
build-time and are listed only where they bake a fact the runtime uses. "Not
read" means no runtime read of that name exists in those paths; a name in a
comment or a test harness does not count. Platform reach was read from the
`cfg` guarding each reader, not run, except where a row cites a measurement.

**Roads.** "Rust" is `nros::main!` on a Rust board; "C/C++" is the
`nros-c`/`nros-cpp` staticlib (its boot resolver `resolve_boot` in
`nros-c/src/support.rs` and `nros-cpp/src/lib.rs`). "Linux" means a Linux
process: the `linux` board (platform `posix`), `threadx-linux`,
`freertos-posix`, and Zephyr `native_sim`, which all compile Rust for the
host triple (`target_os = "linux"`).

### ROS 2 standard variables

| Variable | Read today? Where | Platforms and roads | What `system.toml` states for the same fact |
| --- | --- | --- | --- |
| `ROS_DOMAIN_ID` | Yes. `nros::env::env_rung` (parsed; malformed is `BootConfigError::DomainIdParse`, over 232 is `DomainIdRange`) | Every Linux process, C/C++ road (`resolve_boot`). Rust road: `linux` board (`resolve_hosted`), `threadx-linux` (`nros-board-threadx::entry::resolve_boot`). **Not** the Rust road on `freertos-posix`: `nros-board-freertos::entry` calls `ExecutorConfig::resolve`, no env rung. Never on embedded | `[system] domain_id` (required), `[image.<id>] domain_id`. Baked as `DeployOverlay.domain_id` (Rust), `NROS_ENTRY_DOMAIN_ID` (C), `CONFIG_NROS_DOMAIN_ID` (Zephyr) |
| `ROS_LOCALHOST_ONLY` | Not read | none | Nothing. The middleware file (`rmw/cyclonedds.xml`, `rmw/zenoh.json5`) can state the same restriction |
| `ROS_AUTOMATIC_DISCOVERY_RANGE` | Not read | none | Nothing; same as above |
| `ROS_STATIC_PEERS` | Not read | none | Nothing; the middleware file's peer list (Cyclone `<Peers>`, zenoh `connect/endpoints`) |
| `RMW_IMPLEMENTATION` | Yes, one reader: `nros::init` (`init.rs`, `Context::default_from_env` path) as a fallback for the `Context.rmw` HINT only. Deliberately NOT a backend selector (`nros::env::rmw_selector` doc) | Rust road, `env` feature, Linux. C/C++: not read | `[system] rmw`, `[image.<id>] rmw` (a nano-ros name: `zenoh`, `cyclonedds`, `xrce`) |
| `ROS_NAMESPACE` | Not read. Deliberately (`nros_node` `ExecutorConfig::namespace` doc) | none | The launch file's namespace, baked in the boot blob |
| `ROS_LOG_DIR`, `ROS_HOME` | Not read (the `ROS_HOME` hits are `NROS_HOME`, a CLI store path) | none | Nothing. nano-ros writes no log files |
| `RCUTILS_CONSOLE_OUTPUT_FORMAT`, `RCUTILS_COLORIZED_OUTPUT`, `RCUTILS_LOGGING_USE_STDOUT`, `RCUTILS_LOGGING_BUFFERED_STREAM` | Not read (`RCUTILS_*` appears only in `nros-log` doc comments naming upstream macros) | none | Nothing |
| `ROS_DISABLE_LOANED_MESSAGES` | Not read. A loan primitive exists (`borrow_loaned_message` slot, `nros/rmw_entity.h`) | none | Nothing |
| `ROS_SECURITY_ENABLE`, `ROS_SECURITY_STRATEGY`, `ROS_SECURITY_KEYSTORE`, `ROS_SECURITY_ENCLAVE_OVERRIDE` | Not read | none | Nothing; nano-ros has no SROS2 support |
| `RUST_LOG` (not ROS, but the one log-level variable read) | Yes: `nros-board-linux` stdout sink (`rust_log_default_level`), only when that sink is installed | Rust road, `linux` board only. C/C++ and other boards: not read | Nothing |

### Backend variables

| Variable | Read today? Where | Platforms and roads | `system.toml` equivalent |
| --- | --- | --- | --- |
| `ZENOH_SESSION_CONFIG_URI` | Yes: `nros-rmw-zenoh` `session_config_uri_properties`, through `getenv`, lowered by `nros-zenoh-session-config::lower`; replaces the baked table whole (RFC-0109 D8 amendment, rule 2) | Linux, both roads, when the backend has `session-config-uri` (Rust `std`; C/C++ through the descriptor's `alloc` feature, `nros-rmw.toml`) | `rmw_config` (default `rmw/zenoh.json5`), lowered at build |
| `ZENOH_MULTICAST_SCOUTING`, `ZENOH_SCOUTING_TIMEOUT`, `ZENOH_LISTEN`, `ZENOH_TLS_ROOT_CA_CERTIFICATE`, `ZENOH_TLS_ROOT_CA_CERTIFICATE_BASE64`, `ZENOH_TLS_VERIFY_NAME_ON_CONNECT` | Yes: `nros-rmw-zenoh` `ZenohSession::new` (property defaults) and `append_tls_env_to_locator`, through `std::env` | **Rust road only**: both readers are `#[cfg(feature = "std")]`; the `linux` board enables `nros-rmw-zenoh/std`, a C/C++ image crate names `platform-posix` + `alloc`, not `std`. So a C/C++ Linux process ignores them silently | The same keys in `rmw/zenoh.json5` |
| `ZENOH_CONFIG_OVERRIDE` | Not read by an image. `nros_zenoh_tools` and `nros-orchestration-ir::router_rule` PRODUCE a value for `rmw_zenohd` | none | Nothing |
| `ZENOH_ROUTER_CONFIG_URI` | Not read (a router variable; nano-ros ships no router) | none | Nothing |
| `RMW_ZENOH_*` | Not read. The hits are `rmw_zenoh_cpp`'s QoS default constants named in comments | none | Nothing |
| `ZENOH_LOCATOR`, `ZENOH_MODE` | Refused: `nros::retired_env::RETIRED_ENV_VARS`, checked first in `env_rung`, naming `NROS_LOCATOR` / `NROS_SESSION_MODE` | Linux, every road through `env_rung` | (retired aliases) |
| `CYCLONEDDS_URI` | Yes, two readers. Hosted POSIX: Cyclone's own config loader (third-party). The embedded compose path: `env_lookup` in `session.cpp`, which returns NULL on a freestanding build (`__STDC_HOSTED__ == 0`) | Linux, C/C++ and Rust (the Cyclone backend is one C++ library). On `native_sim` the comment in `session.cpp` says picolibc's `getenv` sees no host environment; not re-measured here | `rmw_config` (default `rmw/cyclonedds.xml`), baked; Cyclone composes the two (a scalar in the env wins, a list element appends: RFC-0109 D8 amendment 3) |

### nano-ros spellings

| Variable | Read today? Where | Platforms and roads | `system.toml` equivalent |
| --- | --- | --- | --- |
| `NROS_LOCATOR` (run time) | Yes: `nros::env::env_rung`. A SET empty value is the env's answer (issue 0330) | As `ROS_DOMAIN_ID` | `[system]`/`[image.<id>] locator` |
| `NROS_LOCATOR`, `NROS_DOMAIN_ID` (BUILD time) | Yes, as `option_env!`: `nros::init::Context::baked`, and the board configs of `freertos`, `mps2-an385`, `nuttx-qemu`, `threadx-linux`, `threadx-qemu-riscv64`; on Zephyr exported from `CONFIG_*` by `nros_zephyr_build::bake_nros_config` | Rust road, embedded and RTOS-on-host boards. A variable of the BUILD's shell, not the process's | Same keys; RFC-0103 D2 makes an exported variable an override at configure |
| `NROS_SESSION_MODE` | Yes: `env_rung`; anything but `peer` is `Client` | As `ROS_DOMAIN_ID` | Only the zenoh file's `mode` (RFC-0109 D8 amendment 1); nothing bakes a mode (`init.rs` `Context::baked` doc) |
| `NROS_RMW` | Yes: `nros::env::rmw_selector` and `env_rung` (one reader, issue 0687); `nros-c/src/executor.rs` reuses `rmw_selector` | As `ROS_DOMAIN_ID` | `rmw`; baked as the boot blob's `rmw` when the image links several backends |
| `NROS_NODE_NAME`, `NROS_NODE_NAMESPACE` | Yes: `env_rung`; empty means unset (issue 0794) | As `ROS_DOMAIN_ID` | The launch file's node name and namespace, baked |
| `NROS_BRIDGE_<N>_LOCATOR`, `NROS_BRIDGE_<N>_DOMAIN` | Yes: `nros-bridge` `config.rs`, `std::env` | The bridge binary (Linux) | `[[domain]] locator` / `id` |
| Operational: `NROS_ENTRY_SPIN_MS`, `NROS_CENSUS_OUT`, `NROS_PROFILE_OUT`, `NROS_PROFILE_INPUTS`, `NROS_GRAPH_DUMP`, `NROS_RMW_TRACE_OPEN`, `NROS_RMW_CYCLONEDDS_SKIP_PREFIX`, `NROS_CYCLONE_MATCH_TIMEOUT_MS`, `XRCE_SERIAL_BAUD`, `NROS_BLESS_PROBE_SIDECAR` | Yes, each through its own reader | Linux | None (tooling and test controls), except `NROS_ENTRY_SPIN_MS`, which is also a build knob (`entry.spin_ms` in the knob registry) |

**One measurement worth keeping.** `NROS_ENTRY_SPIN_MS` alone has seven
runtime readers with four spellings of the read: `std::getenv` in
`rclcpp/detail/main.hpp`, `getenv` in `nros_rtos_run_components.c`,
`std::env::var` in `nros-board-linux` and `nros-bridge`, `env_str` in
`nros-board-threadx-linux`, `nros::env::var` in `nros-cpp`, and `std::env` in
the `nros::main!` expansion. That is the shape CLAUDE.md's "fix the class"
rule describes, and it is why D5 is a gate rather than a convention.

### What the inventory says

1. Every RESPECT-shaped read today is on a Linux process, and the readers
   agree on the five identity fields because they share `env_rung`.
2. Two road gaps: the six `ZENOH_*` property variables are Rust-only, and
   `freertos-posix` reads the environment on its C/C++ road but not its Rust
   road. Neither is reported.
3. No standard ROS 2 variable except `ROS_DOMAIN_ID` (and the
   `RMW_IMPLEMENTATION` hint) is read, and none of the unread ones says so.
4. `NROS_LOCATOR` means two things: a run-time variable on a Linux process
   and a build-time variable for an embedded image.

## Design

### D1 - The rule, and where it applies

For a variable whose disposition is RESPECT, on a process that has an
environment:

```
value = env (when set) > stated (system.toml, as baked) > built-in default
```

The precedence is the existing one (`try_resolve_with`, RFC-0045); this RFC
does not change it. It adds the report: when the environment and the
statement both give a value and the values differ, the process warns once:

```
[WARN] [nros.env]: ROS_DOMAIN_ID=7 overrides system.toml's domain_id=42
```

"Stated" means what the image's build inputs say: `system.toml`, its launch
file (for node name and namespace) and its `rmw_config` file. It never means
a built-in default: issue 2714's `warn_env_overrides_statement` doc already
draws this line ("An RTOS simulator's baked rung is its board config whether
or not anything was stated, so it would report a default as a statement"),
and D4 keeps it by carrying "stated" as its own bit rather than inferring it
from a non-empty baked value.

A process "has an environment" when it is a Linux process with a C library
(`target_os = "linux"`). D3 covers the rest.

### D2 - Disposition per variable

| Variable | Disposition | Reason |
| --- | --- | --- |
| `ROS_DOMAIN_ID` | RESPECT (done by issue 2714) | The user's ruling |
| `NROS_LOCATOR` | RESPECT (done by issue 2714) | Same fact class as the domain: where the image connects |
| `NROS_RMW` | RESPECT, warn on mismatch with `rmw` | An image links only the backends its statement names, so an override can only pick among them; a mismatch is still worth a line |
| `NROS_NODE_NAME`, `NROS_NODE_NAMESPACE` | RESPECT, warn on mismatch with the launch-declared value | Identity is stated by the launch file; overriding it silently is how a graph ends up with two nodes of one name |
| `NROS_SESSION_MODE` | RESPECT, warn when the zenoh file states another `mode` | Only the file states a mode today. No new `system.toml` key: RFC-0109 puts session wiring in the middleware's own file |
| `ZENOH_SESSION_CONFIG_URI` | RESPECT, warn when the image baked a file | Already replaces the baked table whole (RFC-0109 D8); today it does so without a word |
| `CYCLONEDDS_URI` | RESPECT, warn when the image baked a file, saying the two COMPOSE | Cyclone composes them (scalars: env wins; lists: append). The warning must say "composes with", not "overrides", or it misdescribes a loopback file plus a loopback env, the case RFC-0109 measured stopping delivery |
| `ZENOH_MULTICAST_SCOUTING`, `ZENOH_SCOUTING_TIMEOUT`, `ZENOH_LISTEN`, `ZENOH_TLS_*` | RESPECT on both roads (read through the D4 reader, not `std::env`), warn when the zenoh file states the key | Removes the Rust-only gap. See Q4 for retiring them in favour of the stock spelling |
| `NROS_BRIDGE_<N>_LOCATOR`, `NROS_BRIDGE_<N>_DOMAIN` | RESPECT, warn on mismatch with `[[domain]]` | The same rule for a bridge's sessions |
| `ROS_LOCALHOST_ONLY`, `ROS_AUTOMATIC_DISCOVERY_RANGE`, `ROS_STATIC_PEERS` | IGNORE, notice naming the middleware file | Mirroring them is per-backend work (Cyclone config generation; zenoh scouting). No `system.toml` key: discovery scope is middleware wiring, which RFC-0109 gives to the middleware file. See Q2 |
| `RMW_IMPLEMENTATION` | IGNORE as a selector. Notice only when its value maps to a backend other than the image's | A sourced ROS shell sets it for every process; a nano-ros image must not change backend because of it (its vocabulary is ROS names, the selector's is registry names: the reason `rmw_selector` gives). Comparing through a fixed map (`rmw_zenoh_cpp` to `zenoh`, `rmw_cyclonedds_cpp` to `cyclonedds`, `rmw_microxrcedds` to `xrce`) keeps the common case (they agree) silent. The `Context.rmw` hint keeps reading it, through the registry |
| `ROS_NAMESPACE` | IGNORE, notice naming `NROS_NODE_NAMESPACE` | Keeps the existing deliberate choice (`ExecutorConfig::namespace` doc). See Q3 |
| `ROS_LOG_DIR`, `ROS_HOME` | IGNORE, notice | nano-ros writes no log file |
| `RCUTILS_CONSOLE_OUTPUT_FORMAT`, `RCUTILS_COLORIZED_OUTPUT`, `RCUTILS_LOGGING_USE_STDOUT`, `RCUTILS_LOGGING_BUFFERED_STREAM` | IGNORE, notice | `nros_log` has one fixed format and sink per board. See Q5 |
| `ROS_DISABLE_LOANED_MESSAGES` | IGNORE, notice | The loan slot exists but no API path consults this variable. See Q6 |
| `ROS_SECURITY_ENABLE` with `ROS_SECURITY_STRATEGY=Enforce` | REFUSE | Stock ROS 2 refuses to start an enforced node it cannot secure. Running it unsecured while the operator believes otherwise is the one IGNORE that is unsafe |
| other `ROS_SECURITY_*` | IGNORE, notice | No SROS2 |
| `ZENOH_LOCATOR`, `ZENOH_MODE` | REFUSE (unchanged) | Retired aliases, phase-484 W8c |
| `ZENOH_CONFIG_OVERRIDE` | IGNORE, notice, until Q4 is ruled | Stock's spelling for key overrides; nano-ros could lower it with the same reader as the session file |
| `ZENOH_ROUTER_CONFIG_URI` | Listed, no notice | A router's variable. A stock node does not read it either, so a notice in every nano-ros process would be noise. See Q7 |
| `RUST_LOG` | RESPECT on every road with a console sink, not only the Rust `linux` board. No statement, so nothing to mismatch | Today a C/C++ process on the same board ignores it silently |
| Operational `NROS_*` (`NROS_ENTRY_SPIN_MS`, census, profile, trace, dump) | Registered, read through D4, no statement rule | Tooling controls; a `system.toml` equivalent exists only for `NROS_ENTRY_SPIN_MS` (the build knob), and that one gets the mismatch warning |

**Where a variable has no `system.toml` equivalent.** None is proposed. The
candidates (discovery range, session mode, middleware tuning) are
middleware wiring, which RFC-0109 gives to the middleware's own file under
`rmw_config`; logging format has no deployment owner yet (Q5).

### D3 - A process with no environment

An embedded image (FreeRTOS, ThreadX, NuttX, Zephyr on hardware or QEMU, bare
metal) has no process environment. The stated value is the value. On Zephyr
`native_sim` the Rust and C code compile for the host triple and link
`getenv`, but the cited behaviour is that the simulator's libc sees no host
environment (Q8).

A user learns that a variable does nothing in two ways:

1. **Docs.** The book's reference gets one generated table (D4's registry
   rendered), with a "has an environment" column per board family. The
   board catalog states the fact once (a board descriptor field, `process_env
   = true | false`), so the table and the build-time note read one source.
2. **A build-time note.** `nros build` (and `nros configure`) for an image
   whose board has no process environment checks the BUILD's shell for every
   RESPECT row and prints, once per variable:

   ```
   note: ROS_DOMAIN_ID=5 is set in this shell, but image `talker`
         (board mps2-an385) has no process environment: it uses
         system.toml's domain_id=7. Change system.toml, or export
         NROS_DOMAIN_ID=5 to override it for this build.
   ```

   The second sentence exists because the build-time spellings
   (`NROS_DOMAIN_ID`, `NROS_LOCATOR` through `option_env!`) are a real
   override channel today. RFC-0103 D2 already prints and records them as
   `transient`; this RFC adds the same mismatch warning there, so the rule is
   "override, and say so" on both sides of the build.

NuttX is the exception that does not fit "embedded means no environment":
NuttX has `getenv` and an NSH `set`, but `nros::env` compiles only for
`target_os = "linux"`. Q9 asks whether NuttX should RESPECT.

### D4 - One implementation point

**The registry.** One table of every environment variable a nano-ros process
reads, as rows:

```toml
[[env]]
name = "ROS_DOMAIN_ID"
disposition = "respect"          # respect | ignore | refuse | listed
stated = "domain_id"             # the statement it is compared with, or absent
kind = "u32"                     # parse rule; malformed is an error, as today
instead = ""                     # for ignore/refuse: what to use
reader = "nros-env"              # or "third-party:cyclonedds" for a value
                                 # another library reads; nano-ros reads it
                                 # only to compare and report
```

It lives beside the build-time knob registry
(`packages/tooling/nros-zephyr-build/knobs.toml`, RFC-0103 D2b), generated by
the same script (`scripts/gen-knob-registry.py`) into a Rust table, a C
header and the book table. Q1 asks whether it is a section of that file or a
sibling file. It is not a second registry for a fact the knob registry
already holds: a run-time variable that is also a build knob
(`NROS_ENTRY_SPIN_MS`) names its knob row.

**The reader and the decision move one level down.** A new `#![no_std]`
crate, `nros-env` (in `packages/core/`), depends only on `nros-log` and
holds:

- `get(row) -> Option<&'static str>`: the one `getenv` call (the body of
  today's `nros::env::raw`/`var`), compiled to "always unset" where
  `target_os != "linux"`;
- `resolve(row, stated: Option<T>) -> Option<T>`: env > stated, with the
  mismatch warning (today's `env_overrides_statement` plus
  `warn_env_overrides_statement`, generalised from two fields to a row);
- `notice_if_set(row)`: the one-time IGNORE notice; `refuse_if_set(row)` for
  REFUSE;
- a C ABI over the same functions (`nros_env_get`, `nros_env_resolve_str`,
  `nros_env_resolve_u32`, `nros_env_notice_if_set`) and a header
  `<nros/env.h>`, which returns NULL under `__STDC_HOSTED__ == 0` exactly as
  `env_compat.hpp` does today, so the Cyclone C++ backend and the XRCE C
  transport call it.

Every reader in the inventory reaches `nros-env`: the zenoh backend,
`nros-rmw-cffi` and `nros-bridge` already depend on `nros-log`, which is the
only dependency;
the C/C++ backends link the C symbol from the image's staticlib, as they link
`nros_platform_*` today. `nros::env` stays the boot-config ladder: it builds
the `EnvRung` from `nros-env` rows and keeps `report`. Issue 2714's
`StatementOverrides` becomes a call per row.

**The warning.** `nros_log::log_warn!` on logger `nros.env` (a child of 2714's
`nros`, RFC-0102), so it reaches every board's console sink and is filtered
like every other nano-ros log line; never a raw `stderr` write, which on
`native_sim` is issue 0589's hazard. A notice is `log_info!`. Each row has
one bit in a static atomic bitset, so a row warns at most once per process,
however many sessions or tiers resolve it (a levels runner resolves once per
level today). A warning carries an `NROS-NNNN` diagnostic code (RFC-0129) so
`nros explain` can say more than one line can; Q10.

**Startup sweep.** The first resolve in a process also walks the IGNORE and
REFUSE rows, so a set-but-ignored variable is reported whether or not any
code path would have read it.

### D5 - The gate

`env-reads-through-registry` (a test in `packages/testing/nros-gates`,
RFC-0131), on the fast line:

1. Every runtime environment read in tracked sources (`getenv(`,
   `std::getenv`, `env::var(`, `env::var_os(`, `env_lookup(`, a raw
   `extern "C" fn getenv` declaration) outside `nros-env` itself fails, with
   the file and the variable name, naming `nros_env::get`. Build scripts,
   `packages/cli`, `packages/tooling`, test code and `scripts/` are out of
   scope: they read the BUILD's environment, which RFC-0103 governs.
2. Every name passed to `nros_env::get` / `nros_env_get` is a registry row.
3. Every registry row is read somewhere, or is `listed`/`ignore`/`refuse`
   (a registered RESPECT row with no reader is the unwired-producer shape).
4. `option_env!` of a registry name is allowed only in the files the
   registry lists for its build-time spelling.

A self-test plants one bypassing read in a temporary tree and asserts the
gate names it (the negative control every gate here carries). The reach
check (0196's rule): the gate's file set is derived from `git ls-files`
over the runtime crates, not authored.

### D6 - Stock ROS 2 semantics, and where nano-ros differs

These are divergences in RFC-0036's sense; each gets a row there when it
lands.

- **`ROS_LOCALHOST_ONLY` and `ROS_AUTOMATIC_DISCOVERY_RANGE`.** In Jazzy,
  `ROS_LOCALHOST_ONLY` is deprecated in favour of
  `ROS_AUTOMATIC_DISCOVERY_RANGE` (`SUBNET`, `LOCALHOST`, `OFF`,
  `SYSTEM_DEFAULT`) plus `ROS_STATIC_PEERS`; Humble has only the former.
  nano-ros supports both editions, so if these are ever RESPECTed, both
  spellings must be, with Jazzy's precedence. This RFC proposes IGNORE first:
  the harness already learned (issue 1009) that `ROS_LOCALHOST_ONLY` reaches
  ROS processes only, and that a half-isolated pair finds nothing. A notice
  is better than a partial imitation.
- **`RMW_IMPLEMENTATION`.** Stock uses it to load an RMW library at run
  time. A nano-ros image links its backends at build time, so the variable
  cannot select anything the image does not contain; nano-ros uses
  `NROS_RMW` among linked backends and reports a disagreeing
  `RMW_IMPLEMENTATION`.
- **`ROS_DOMAIN_ID` on embedded.** Stock always reads it; an embedded
  nano-ros image cannot. D3's note is the replacement.
- **Logging variables.** Stock `rcutils` reads the four `RCUTILS_*`
  variables and writes log files under `ROS_LOG_DIR` (default
  `$ROS_HOME/log`). nano-ros has neither a configurable console format nor
  file logging.
- **`ROS_SECURITY_*`.** Stock enforces; nano-ros refuses the enforced case
  rather than run unsecured.
- **Precedence.** Stock has no `system.toml`; a stock node takes the
  environment or its compiled default. nano-ros adds the statement rung
  between them, and the warning is how a ROS user who expects "env decides"
  sees that a statement existed.

## Alternatives considered

- **Keep the decision in `nros::env` and have each backend re-implement it.**
  Rejected: that is a spelling per language and per crate, which the
  inventory shows already produced a Rust-only gap and seven readers of one
  variable.
- **The statement always wins; the environment is refused when it
  disagrees.** Rejected by the user's ruling: on a Linux host the
  environment is how tests and one-off runs pick a domain.
- **Mirror every stock variable.** Rejected for now: several need per-backend
  work (discovery range on two backends), and a partial imitation is worse
  than a stated IGNORE.
- **A notice for every unknown `ROS_*` variable.** Rejected: a sourced ROS
  shell sets `ROS_DISTRO`, `ROS_VERSION`, `ROS_PYTHON_VERSION`, which no node
  acts on. Only registry rows are reported.

## Non-goals

- Command-line `--ros-args` (remaps, `__ns`, `__node`, `--log-level`,
  `--params-file`): argv, not the environment; `nros::init_with_args`
  owns it.
- The CLI's and build scripts' own variables (`NROS_HOME`, `NROS_BOARD`,
  `FREERTOS_PORT`, the `NROS_*` build knobs): RFC-0103.
- Making a launcher write the environment from `system.toml`: issue 2714
  made the bake carry the statement, so no launcher needs to.

## Decisions (user ruling, 2026-11-19)

The user accepted the recommendations on Q2 to Q11 below. Q1 is reopened
with a wider scope and is not decided yet.

1. **Registry home: REOPENED, scope widened.** Environment variables are to
   be designed TOGETHER WITH the knobs: one configuration source of truth and
   one naming scheme across `system.toml` keys, `NROS_*` build carriers,
   `CONFIG_NROS_*` Kconfig symbols and run-time environment variables. So an
   environment variable becomes a row of the same knob registry, not a row of
   a sibling `env-vars.toml`. The ruling also asks that a custom board package
   and a custom RMW package be self-contained, colcon-style packages that can
   declare their own knobs and environment variables without editing central
   in-tree files. D4 and D5 will be revised against a survey of today's
   registries; until then they describe the narrower design.
2. **Discovery range: IGNORE with a notice** for `ROS_LOCALHOST_ONLY`,
   `ROS_AUTOMATIC_DISCOVERY_RANGE` and `ROS_STATIC_PEERS`. RESPECT on the
   Cyclone backend is later work, not this phase.
3. **`ROS_NAMESPACE`: match stock rclcpp**, after verifying what rcl and
   rclcpp actually do with it. The `ExecutorConfig::namespace` doc's claim
   is corrected to whatever the measurement says.
4. **nano-ros' own `ZENOH_*` variables: RETIRE.** The six inventions
   (`ZENOH_MULTICAST_SCOUTING`, `ZENOH_SCOUTING_TIMEOUT`, `ZENOH_LISTEN`,
   three `ZENOH_TLS_*`) are REFUSED, each naming its replacement, the stock
   `ZENOH_CONFIG_OVERRIDE`, lowered by the same reader as the session file.
5. **Logging: RESPECT** `RCUTILS_COLORIZED_OUTPUT` and
   `RCUTILS_CONSOLE_OUTPUT_FORMAT` in the Linux console sink. `RUST_LOG`
   stays a Rust-only variable.
6. **Loans: IGNORE** `ROS_DISABLE_LOANED_MESSAGES`.
7. **Router variables:** `ZENOH_ROUTER_CONFIG_URI` gets a notice like every
   other ignored row.
8. **`native_sim`: measure** whether its libc sees the host environment,
   then correct whichever of RFC-0109 D6a and the Cyclone backend's comment
   is wrong.
9. **NuttX: RESPECT.** The reader is compiled for `target_os = "nuttx"`.
10. **Diagnostic codes: plain log lines first.** `NROS-NNNN` codes come
    later, under RFC-0129.
11. **`freertos-posix` Rust: RESPECT**, as `threadx-linux` does through
    `resolve_boot`, so its Rust road agrees with its C/C++ road.

## Related

- RFC-0045 (one boot-config resolver), RFC-0103 D2 (an override is not a
  source), RFC-0109 D6a and D8 (run-time overrides on hosted builds; the
  middleware's own file), RFC-0036 (divergences), RFC-0102 (logger
  hierarchy), RFC-0129 (diagnostic codes), RFC-0131 (gates are tests).
- Issue 2714 and PR #4285 (the domain and locator); issues 0687 (one env
  reader), 0794, 1009, 1434.

## Changelog

- 2026-11-19: Draft.
- 2026-11-19: Q2 to Q11 decided by the user; Q1 reopened (env vars join the
  knob registry under one naming scheme; self-contained board and RMW
  packages).
