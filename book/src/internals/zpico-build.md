# zpico-sys Build Architecture

`zpico-sys` is nano-ros's Rust `*-sys` crate for zenoh-pico. After
Phase 136 (2026-05-18) the build path is a single `cc-rs` invocation
driven entirely by a TOML manifest. Earlier phases shipped a
CMake/cc-rs hybrid with per-RTOS Rust functions; the unified path
collapses that surface to one consumer + one data file.

## The manifest

`packages/zpico/zpico-sys/zenoh_platforms.toml` declares every
per-platform datum the build needs. Two top-level table groups:

- `[platform.<name>]` — one block per supported platform
  (`posix`, `zephyr`, `freertos-lwip`, `nuttx`, `threadx`,
  `bare-metal`, `generic`, `orin-spe`).
- `[arch.<name>]` — reusable target-arch compiler-flag profiles
  shared across platforms that target the same CPU family
  (`cortex-m3`, `cortex-m4f`, `cortex-a7`, `cortex-r5-softfp`,
  `riscv32imc`, `riscv32gc`, `riscv64gc`).

### Per-platform fields

| Field | Type | Use |
|-------|------|-----|
| `inherits` | string | Merge from another `[platform.*]` block before applying this one. |
| `defines` | list[str] | Unconditional `cc::Build::define(name, None)`. |
| `defines_kv` | map | `cc::Build::define(name, Some(value))`. |
| `defines_env` | map | Value from `env`, falls back to `default`. |
| `include` | list[str] | Glob roots under `zenoh-pico/src/`. The drift gate validates these exist. |
| `exclude` | list[str] | Drop entries from `include` matches. |
| `extra_sources` | list[ExtraSource] | Additional `.c` files outside `zenoh-pico/src/`. See below. |
| `required_env` | list[RequiredEnv] | SDK paths the build needs. See below. |
| `include_paths` | list[str] | Header search paths; interpolated. |
| `include_paths_conditional` | list[ConditionalPath] | Header paths gated by `when`. |
| `arch` | string \| list[str] | Name(s) of `[arch.*]` block(s) to apply. Single name (TOML scalar) for single-arch platforms; list (TOML array) for multi-arch platforms like `bare-metal` (cortex-m3 + riscv32imc). build.rs walks the list in order and applies the first arch whose `target_match` hits the build target. Phase 148. |
| `compile` | table | `opt_level` / `warnings` / `cflags`. |
| `pic` | bool | `cc::Build::pic` override (NuttX flat builds use `false`). |
| `link.*` | map | Per-link-feature policy (Phase 134). Values: `true` / `false` (force on/off) or `"feature"` (defer to `CARGO_FEATURE_LINK_<X>`). |
| `mbedtls` | string | `pkg-config` / `vendored` / `none`. |
| `system_libs` | list[str] | `cargo:rustc-link-lib=` entries. |
| `rerun_if_env_changed` | list[str] | `cargo:rerun-if-env-changed=…` triggers. |

### Per-arch fields

| Field | Type | Use |
|-------|------|-----|
| `target_match` | string | Substring or `<prefix>*` glob the target triple must match. |
| `target_exclude` | string | Veto when the target triple contains this substring (e.g. `cortex-m3` excludes `thumbv7em` so Cortex-M4 doesn't pick it). |
| `cflags` | list[str] | Compiler flags. |
| `needs_picolibc` | bool | Add picolibc sysroot's `include/` to the search path. |
| `needs_errno_override` | bool | Generate + prepend the errno-override shadow header (RISC-V picolibc TLS-errno workaround). |
| `needs_riscv_compiler` | bool | Probe for a RISC-V cross-cc. |

### Interpolation tokens

Every `path` / `include_paths` / `extra_sources.path` /
`include_paths_conditional.path` string is run through a small
interpolator before being passed to `cc-rs`:

| Token | Value |
|-------|-------|
| `{nros}` | `CARGO_MANIFEST_DIR` (`zpico-sys/`). |
| `{out}` | `OUT_DIR`. |
| `{src}` | `zenoh-pico/src/` under `{nros}`. |
| `{env:VAR}` | Value of env var `VAR`; build fails with a clear error if unset. |

### `when` matcher

Conditional includes carry a `when` table. Each field is optional;
present fields ALL must hold for the matcher to fire.

| Field | Meaning |
|-------|---------|
| `target_match` | Substring (or `<prefix>*` glob) that must be in the target triple. |
| `target_not` | Substring that must NOT be in the target triple. Special value `"embedded"` matches any of the known embedded RTOSes. |
| `if_env` | Env var that must be set (any value). |

### `ExtraSource` shape

```toml
extra_sources = [
  { path = "{nros}/c/platform/threadx/task.c" },
  { path = "{nros}/c/platform/threadx/log_uart.c",
    if_env = "NROS_ZPICO_LOG_TO_UART",
    with_define = ["ZENOH_LOG_PRINT", "zpico_log_print"] },
]
```

`if_env` skips the file when the env var is absent. `with_define`
adds the matching `cc::Build::define(name, Some(value))` whenever
the source is included (use `name` alone for unconditional `None`).

### `RequiredEnv` shape

```toml
required_env = [
  { name = "THREADX_DIR",
    help = "ThreadX kernel source. just setup-threadx; export THREADX_DIR=$PWD/third-party/threadx/kernel",
    validate_subdir = "common/inc" },
]
```

Missing env vars panic at build time with the `help` string;
present env vars whose value lacks the `validate_subdir` subdir
also panic with the offending path printed. No silent fallthroughs.

## Adding a new platform

Adding a new RTOS is a TOML edit:

1. Pick an `[arch.<name>]` block for the target CPU. If the family
   isn't already there (or compiler flags differ), add an
   `[arch.*]` block. Reusable across platforms.
2. Write the `[platform.<your_rtos>]` block with `defines` +
   `include` + `extra_sources` + `required_env` + `include_paths`.
3. The Cargo feature for the new platform must be added in
   `Cargo.toml` and wired into the existing match in `build.rs`
   that maps Cargo feature → manifest platform name.
4. Verify with `cargo build -p zpico-sys --features <your_rtos>
   --target <triple>`.

No `build.rs` edits needed for the data layer.

## Adding a new target arch

Adding a new CPU family is one `[arch.*]` block + any of the
`needs_picolibc` / `needs_errno_override` /
`needs_riscv_compiler` flags it requires. Existing platforms that
need to target it set `arch = "<your_arch>"` (or extend their
existing `arch = [...]` list with the new name).

For platforms that span multiple architectures (e.g. `bare-metal`
covers `cortex-m3` for `qemu-arm-baremetal` / `stm32f4` AND
`riscv32imc` for `ESP32-C3`), declare every arch the platform
supports and let build.rs's first-match dispatch pick the right
one per target triple:

```toml
[platform.bare-metal]
arch = ["cortex-m3", "riscv32imc"]   # first arch_matches wins
```

This is what makes `cargo check` work on both
`qemu-arm-baremetal/rust/zenoh/talker` and
`esp32/rust/zenoh/{listener,talker}` from the same platform
entry — the picolibc sysroot wired up by
`arch.riscv32imc.needs_picolibc = true` is added to the cc-rs
`-I` list only when the build target is riscv32imc-*. (Phase 148.)

## mbedTLS source policy

Manifest's `mbedtls` field per-platform:

- `pkg-config` — discover via the `pkg-config` build crate. POSIX
  hosts only. The build script also synthesizes `.pc` files when
  the host doesn't ship them (Ubuntu's `libmbedtls-dev` is the
  motivating case).
- `vendored` — pull from the in-tree `mbedtls/` submodule sources.
  Bare-metal / RTOS targets without a system mbedTLS.
- `none` — `link-tls` users on this platform get a link error;
  the platform doesn't support TLS.

The selection only matters when `link-tls` is on (controlled by
the `CARGO_FEATURE_LINK_TLS` env var).

## Source-drift gate

Every `include` root in `zenoh_platforms.toml` must (a) resolve to
a real directory under `zenoh-pico/src/`, (b) contain `≥1 .c` file
or sub-directory. A failed check panics build-time with the
offending key + expected path. Phase 136.6 owns this; full
set-equality vs. the resolved cc-rs source list lands as a
follow-up.

## Manifest-driven consumer

`build.rs`'s `build_zenoh_pico_unified` consumes a
`ResolvedPlatform` (the merged `inherits` chain). It:

1. Validates `required_env` + `validate_subdir`.
2. Generates the version header in `{out}/zenoh-pico-version/`.
3. Applies the `[arch.*]` profile if its `target_match` /
   `target_exclude` predicates pass.
4. Adds core sources (8 protocol subdirs + `system/common`) and
   per-platform `extra_sources` (with `if_env` / `with_define`).
5. Sets include paths (unconditional + conditional after `matches`).
6. Adds defines (`defines` / `defines_kv` / `defines_env`).
7. Handles mbedTLS per the manifest's `mbedtls` field.
8. Applies shim slot counts.
9. Applies compile settings + `pic` override.
10. Compiles to `libzenohpico.a`.
11. Registers `rerun_if_env_changed`.

The consumer is the only function that actually invokes `cc-rs`
for zenoh-pico. Every per-platform delta lives in the TOML.

## Related

- `docs/roadmap/phase-136-zpico-sys-unified-build.md` — the phase
  doc that drove this refactor.
- `book/src/concepts/platform-model.md` — Boards vs Platforms;
  manifest is the platform-side knob.
- `book/src/internals/rmw-backends.md` — RMW host-language policy.
