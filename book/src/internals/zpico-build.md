# zpico-sys Build Architecture

`zpico-sys` is nano-ros's Rust `*-sys` crate for zenoh-pico. Its build script
is one line; the work is in `packages/rmw/zenoh/nros-zpico-build`
(`runner::run`), a library so its parsing and source selection can be
unit-tested outside Cargo's build-script harness. It compiles the vendored
zenoh-pico C with a single `cc-rs` invocation, driven by data rather than by
per-RTOS Rust code.

Zephyr is the exception: its west/cmake lane compiles the same vendored tree
through `zephyr/cmake/nros_rmw_zenoh.cmake`, and the cargo build of
`zpico-sys` for a Zephyr image compiles none of it (see `compiled_by` below).

## Where the data lives

Three files, each owned by the thing it describes. None of them is the
`zenoh_platforms.toml` older docs mention; that central manifest was split up
by phase-290 and phase-349 W2b.

| What | File | Owner |
|------|------|-------|
| The core zenoh-pico sources, every platform | `packages/rmw/zenoh/zpico-sys/zenoh-sources.txt` | the backend (phase-420 W9) |
| zenoh-pico's per-port defines, extra sources, include paths | `[port.<platform>]` in `packages/rmw/zenoh/nros-rmw-zenoh/nros-rmw.toml` | the backend |
| CPU flag profiles, compile settings, required SDKs | each platform's `nros-platform.toml` | the platform |

A platform names no backend (RFC-0072 §9): what a platform needs from **any**
vendored C build stays in its own file, and what zenoh-pico needs **on** that
platform is a row in zenoh's descriptor. A build joins the two.

### `zenoh-sources.txt`

The one source list, read by both the cargo lane and the cmake lane. Records
are `group <name> <condition>`, `dir <group> <tree> <path>` (every `.c` under
`<path>`, recursively) and `src <group> <tree> <path>` (exactly one file). The
core is nine `dir` records under `zenoh-pico/src/`, condition `always`; the
two other groups are Zephyr-only. The file's header explains the format and
why it lists directories rather than files. Gate:
`just check zenoh-source-manifest` — every `.c` under `zenoh-pico/src/` must
be covered by a record or sit under a `system/<platform>/` tree some port row
selects, so a new upstream DIRECTORY fails the gate.

### The platform half — `nros-platform.toml`

Seven files: `config/{bare-metal,generic}/` and
`packages/platform/nros-platform-{freertos,nuttx,posix,threadx,zephyr}/`.
Most of each file is the RFC-0049 knob ladder (`[capabilities]`, `[knobs.*]`);
the C-build keys are:

| Key | Type | Use |
|-----|------|-----|
| `names` | list[str] | Names this platform answers to; `names[0]` is canonical (`config/bare-metal` answers `bare-metal` and `esp32`). |
| `inherits` | string | Parent platform. Its C-build keys and its port row merge underneath this one (rules below). |
| `arch_profiles` | str \| list[str] | Which `[arch.*]` profiles apply, in order; the first whose `target_match` admits the target wins. |
| `compile` | table | `opt_level` / `warnings` / `cflags`. |
| `required_env` | list[RequiredEnv] | SDK paths the build needs. See below. |
| `[arch.<name>]` | table | A flag profile (below). A profile may appear in several files only if every copy is identical. |

Profiles listed today: `cortex-m3`, `cortex-m4f`, `riscv32imc` (bare-metal);
`cortex-m3`, `cortex-m7`, `cortex-r5`, `cortex-r52` (FreeRTOS); `cortex-a7`
(NuttX); `riscv64gc` (ThreadX). `posix`, `zephyr` and `generic` list none.
Gate: `just check arch-profile-resolution` checks every supported triple
against these lists.

The search path is `$NROS_PLATFORMS_DIR` (colon-separated, searched first),
then `packages/platform/`, then `config/`. An empty tree is fatal, not a
fallback to builtins.

#### `[arch.<name>]`

| Key | Type | Use |
|-----|------|-----|
| `target_match` | string | Substring or `<prefix>*` glob the target triple must match. |
| `target_exclude` | string | Veto when the triple contains this substring (`cortex-m3` excludes `thumbv7em`, so a Cortex-M4 does not pick it). |
| `cflags` | list[str] | Compiler flags. |
| `needs_picolibc` | bool | Add the picolibc sysroot's `include/`. |
| `needs_errno_override` | bool | Generate and prepend the errno-override shadow header (RISC-V picolibc TLS-errno workaround). |
| `needs_riscv_compiler` | bool | Probe for a RISC-V cross `cc`. |

### The backend half — `[port.<platform>]`

Rows exist for `freertos`, `nuttx`, `posix`, `threadx`, `zephyr`,
`bare-metal` and `generic`; the key is a platform name or alias. A row is
`deny_unknown_fields`: one that names `arch_profiles`, `compile` or
`required_env` is a platform fact in the wrong file and fails to load. The
schema is `PortRow` in `packages/tooling/nros-platform-config/src/manifest.rs`.

| Key | Type | Use |
|-----|------|-----|
| `defines` | list[str] | `define(name, None)`. |
| `defines_conditional` | list[{name, when}] | A define gated by a [`when`](#when-matcher) matcher. FreeRTOS uses it to pick `ZENOH_FREERTOS_LWIP` or `ZENOH_ORIN_SPE` from the board's `ip_stack` capability. |
| `defines_kv` | map | `define(name, Some(value))`. |
| `defines_env` | map of {env, default} | Value from env var `env`, else the literal `default`. |
| `include` | list[str] | Directories under `zenoh-pico/src/` this port relies on. **Not** a source selection — that is `zenoh-sources.txt` plus `extra_sources`. The build checks each exists and holds a `.c` file, and watches it for rebuilds: the drift check against a submodule bump. |
| `extra_sources` | list[ExtraSource] | `.c` files to compile on this port, e.g. `{src}/system/unix/network.c`. See below. |
| `include_paths` | list[str] | Header search paths, interpolated. Order matters: first wins. |
| `include_paths_conditional` | list[{path, when}] | Header paths gated by `when`. |
| `mbedtls` | string | `pkg-config` / `vendored` / `none`. Read only when the `link-tls` feature is on. |
| `pic` | bool | `cc::Build::pic` override (NuttX flat builds set `false`). |
| `rerun_if_env_changed` | list[str] | Extra `cargo:rerun-if-env-changed` triggers. VALUE knobs only, never a path variable (issue 0491). |
| `compiled_by` | `cargo` \| `platform` | `platform` means the platform's own build compiles this C and cargo must not; the row is then descriptive, and still checked. Only `zephyr` sets it (issue 0534). |

The `system_libs`, `exclude` and `link` keys that older docs list were parsed
and read by nothing. They were removed (issue 2031), so a row naming one is
refused. `pthread` on POSIX is emitted by the runner itself.

## How a build resolves its platform

1. **Pick the name.** From `zpico-sys`'s Cargo feature: `threadx`, `nuttx`,
   `freertos`, `zephyr`, `bare-metal`. With none of those, a hosted target
   (Linux, the BSDs, Android) is `posix`; macOS must pass the feature.
2. **Join.** `PlatformsTree::as_platform_manifest` builds one `PlatformEntry`
   per platform file: its port row (or an empty one) plus the file's
   `inherits`, `arch_profiles`, `compile` and `required_env`. A row that names
   no platform, or two rows that name the same one, is an error.
3. **Merge the `inherits` chain**, parent first (`PlatformManifest::for_platform`):
   - lists append, parent's entries first;
   - maps (`defines_kv`, `defines_env`) take the child's value per key;
   - `mbedtls`, `pic`, `compiled_by` and each `compile` field take the child's
     value if set, else the parent's, so an unset child never downgrades a
     parent;
   - `arch_profiles` is replaced, not appended, when the child sets it.
4. **Check every platform**, not just the selected one: each must resolve, and
   each port's `include` directories must exist. A bad row anywhere fails
   every zenoh build, which is what keeps the ports nobody is building honest.

## Shared shapes

### `when` matcher

Every present field must hold; an empty matcher is always true.

| Field | Meaning |
|-------|---------|
| `target_match` | Substring (or `<prefix>*` glob) that must be in the target triple. |
| `target_not` | Substring that must NOT be in the triple. The special value `"embedded"` means "the target is one of the known embedded RTOSes". |
| `if_env` | Env var that must be set (any value). |
| `capability` | Map of RFC-0086 facts that must be declared with the given value, resolved through the RFC-0049 ladder (platform, then board, board winning). An undeclared fact matches neither `true` nor `false`. The one gate that can differ between two boards of one platform (issue 1143). |

### `ExtraSource`

```toml
extra_sources = [
  { path = "{src}/system/freertos/system.c" },
  { path = "{src}/system/freertos/lwip/network.c",
    when = { capability = { ip_stack = true } } },
  { path = "{nros}/c/platform/threadx/log_uart.c",
    if_env = "NROS_ZPICO_LOG_TO_UART",
    with_define = ["ZENOH_LOG_PRINT", "zpico_log_print"] },
]
```

`when` and `if_env` both gate the file. `with_define` is `[name]` or
`[name, value]`, defined whenever the file is compiled.

### `RequiredEnv`

```toml
required_env = [
  { name = "THREADX_CONFIG_DIR",
    help = "Directory containing tx_user.h. Example: packages/boards/nros-board-threadx-linux/config" },
]
```

A missing variable panics with `help`. With `validate_subdir`, a present
variable whose value lacks that subdirectory panics naming the path. An entry
with a `when` is required only where the matcher holds, so a FreeRTOS board
with no network stack does not need an lwIP SDK.

### Interpolation tokens

Paths in `extra_sources`, `include_paths` and `include_paths_conditional`
pass through `manifest::interpolate`:

| Token | Value |
|-------|-------|
| `{nros}` | `zpico-sys`'s `CARGO_MANIFEST_DIR`. |
| `{out}` | `OUT_DIR`. |
| `{src}` | `zenoh-pico/src/` under `{nros}`. |
| `{env:VAR}` | Value of `VAR`; an unset variable fails the build. |
| `{envpath:VAR}` | A path-valued env var, resolved through the checkout-path rule (issue 1280). |
| `{source:NAME}` | A vendored source tree by name, located through `nros locate` (RFC-0103 D4). |
| `{nuttx_include}` | The NuttX include root for this arch's export. |
| `{freertos_port_dir}` | The FreeRTOS port directory `FREERTOS_PORT` names, mapped through upstream's `portable/CMakeLists.txt` table (phase-349 W3). |

## The compile

`build_zenoh_pico_unified` in `nros-zpico-build/src/runner.rs` is the only
place that invokes `cc-rs` for zenoh-pico, and it is skipped when the row
says `compiled_by = "platform"`. In order:

1. Validate `required_env` (honouring `when`).
2. Generate the version header into `{out}/zenoh-pico-version/`.
3. Apply the first `[arch.*]` profile from `arch_profiles` that admits the
   target.
4. Add the `zenoh-sources.txt` groups this lane selects, then the row's
   `extra_sources` (`when`, `if_env`, `with_define`).
5. Add include paths: the generated config, zenoh-pico's own, the version
   header and the platform C ABI headers, then the row's paths
   (unconditional, then conditional).
6. Add defines: `defines`, `defines_conditional`, `defines_kv`,
   `defines_env`.
7. Handle mbedTLS per `mbedtls`, when `link-tls` is on.
8. Apply shim slot counts and `compile` settings, then the `pic` override.
9. Compile `libzenohpico.a` and register `rerun_if_env_changed`.

The zpico shim (`c/zpico/zpico.c`) is a separate compile in `build_c_shim`,
and shares the generated zenoh config header with the library (issue 0135).

## Adding a platform

1. **The platform file.** Create `nros-platform.toml` beside the platform
   crate (or in a directory on `$NROS_PLATFORMS_DIR`): `names`, and the
   C-build keys if the platform cross-compiles. Reuse an existing `[arch.*]`
   profile by naming it in `arch_profiles`, or define one.
2. **The port row.** Add `[port.<name>]` to `nros-rmw-zenoh/nros-rmw.toml`:
   the `ZENOH_*` platform define, the `system/<x>/` sources as
   `extra_sources`, and include paths. If the platform's own build compiles
   zenoh-pico, set `compiled_by = "platform"`.
3. **The feature.** Add the `zpico-sys` Cargo feature and its arm in
   `runner.rs`'s `platform_name` derivation.
4. **Check.** `cargo build -p zpico-sys --features <name> --target <triple>`,
   then `just check zenoh-source-manifest` and
   `just check arch-profile-resolution`.

A new CPU family is one `[arch.*]` block in each platform file that admits
it, plus its name in their `arch_profiles`. A multi-arch platform lists every
profile and lets the first match pick: `bare-metal` lists `cortex-m3`,
`cortex-m4f` and `riscv32imc`, so one platform serves mps2-an385 and the
ESP32-C3.

## Related

- `docs/roadmap/archived/phase-136-zpico-sys-unified-build.md` — the unified
  `cc-rs` build.
- `docs/roadmap/archived/phase-349-rtos-integration-shells.md` — W2b, the
  platform/backend split.
- RFC-0049 (`docs/design/0049-hierarchical-platform-board-config.md`) — the
  platform and board files and their ladder.
- `book/src/concepts/platform-model.md` — boards vs platforms.
- `book/src/internals/rmw-backends.md` — RMW host-language policy.
