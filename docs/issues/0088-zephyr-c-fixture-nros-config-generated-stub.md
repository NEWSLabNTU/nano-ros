---
id: 88
title: C consumer compiles against the nros_config_generated.h stub (per-build header not ordered/on the include path)
status: open
type: bug
area: cmake
related: [phase-258, 0086, 0087]
---

## Resolution status (2026-06-19)

- **Native / cpp / mixed: FIXED** (commit pending) — the in-tree mirror in
  `packages/core/nros-{c,cpp}/CMakeLists.txt` is now a first-class `OUTPUT` +
  `nros_{c,cpp}_config_header` custom target, and `NanoRosNodeRegister.cmake`
  deps every `${_NRC_SOURCES}` consumer (component lib + carrier executables) on
  the header generators via a DEFERRED `add_dependencies` (so the edge applies
  even when the consumer is configured before the generator subdir). Verified:
  native cpp + mixed workspace fixtures build clean, repeatedly.
- **Zephyr: NOT fixed — separate, deeper integration race (see below).** Kept
  open, area reassigned `cmake`/zephyr.

## Zephyr residual (the hard part)

On Zephyr the per-build header is wired differently: `zephyr/CMakeLists.txt`
builds nros-c/nros-cpp via its own `nros_cargo_build` macro (targets
`nros_{c,cpp}_cargo_build`, NOT Corrosion's `cargo-build_nros_{c,cpp}`) and adds
the generated dir + the source stub dir via `zephyr_include_directories(...)`,
which applies to `app` / zephyr_library targets — **but a `nano_ros_node_register`
component is a plain `add_library`, so it does NOT inherit that include wiring**.
Its `<nros/nros_config_generated.h>` resolves through whatever it links, and it
races the generator (`add_dependencies` on `nros_c_cargo_build` — now added — was
insufficient; the include-dir wiring itself is the gap). Compounding: the 4.4
host-gcc line also hits an unrelated `zephyr/sys/util_internal_is_eq.h` macro
parser error (host gcc vs Zephyr 4.4 headers) and the rustup-target race
([[0086-zephyr-fixture-rustup-target-race]]). Zephyr host fixtures need their own
pass; the native fix here does not close them.

## Symptom (2026-06-19)

`just zephyr build-fixtures` (observed on the 4.4 host-gcc line) — the **C**
fixture `build-c-listener-zenoh` fails compiling the example TU even though its
`nros-c` cargo build finished fine:

```
build-c-listener-zenoh.log:
  ... Compiling nros-c v0.5.0 ...
      Finished `nros-fast-release` profile [optimized + debuginfo] target(s) in 1m 09s
  FAILED: CMakeFiles/nros_zephyr_listener_c_listener_component.dir/src/Listener.c.obj
  nros/nros_config_generated.h:29:2: error: #error "nros_config_generated.h must be
      supplied per-build by the build system; see this stub for guidance."
  nros/nros_generated.h:940:20: error: 'SESSION_OPAQUE_U64S' undeclared here (not in a function)
  nros/nros_generated.h:1031:20: error: 'EXECUTOR_OPAQUE_U64S' undeclared here
  ... (every *_OPAQUE_U64S + ActionServerRawHandle undeclared)
  ninja: build stopped: subcommand failed.
```

So `Listener.c` is compiled against the **stub** `nros_config_generated.h`
(the `#error` placeholder) rather than the real per-build header — hence every
`*_OPAQUE_U64S` storage-size constant + `ActionServerRawHandle` is undeclared.

## Expected

`packages/core/nros-c/CMakeLists.txt:142-159`: `nros-c`'s `build.rs` writes the
per-build `nros_config_generated.h` into `${CORROSION_BUILD_DIR}` and a custom
command **mirrors** it to the in-tree include dir
(`${_nros_c_intree_include_dir}/nros/nros_config_generated.h`) for consumers that
`#include <nros/nros_config_generated.h>`. The C example TU must see the
generated header, not the committed stub.

## Root cause (refined 2026-06-19, after installing clang)

Two layers — one was a clang-prereq symptom, one is the real bug:

1. **(was)** With libclang's builtin headers missing, `nros-c`'s build.rs bindgen
   failed (`stddef.h not found`) → `nros_config_generated.h` was never produced →
   *every* C fixture saw the stub. **Fixed by installing `clang`** (the doctor
   `apt-packages` prereq now covers it, commit `5cd7359e1`). Post-clang: all Rust
   fixtures + most C fixtures build, `stddef.h` errors = 0.

2. **(real bug, remains)** The in-tree header mirror is a `POST_BUILD` custom
   command on `cargo-build_nros_c` (`packages/core/nros-c/CMakeLists.txt:154-161`):
   it `copy_if_different`s `${CMAKE_CURRENT_BINARY_DIR}/nros_config_generated.h` →
   `${_nros_c_intree_include_dir}/nros/nros_config_generated.h`. But the **C app TU
   has no dependency edge on that mirrored byproduct**, so the app object
   (`AddTwoIntsServer.c.obj`) can be scheduled *before* the POST_BUILD mirror runs
   and reads the committed **stub** header. Order-dependent / intermittent: in one
   serial `just zephyr build-fixtures` run `build-c-{listener,talker}` won the order
   and passed, while `build-c-service-server` lost and failed with the stub
   (`SESSION_OPAQUE_U64S undeclared`, etc.).

## Fix direction

Give the mirrored header a real producer→consumer edge so no C TU compiles before
it exists: e.g. an `add_custom_command(OUTPUT …)` + `add_custom_target` for the
mirror that the C-consumer targets `add_dependencies` on (or `target_sources` the
generated header), instead of a `POST_BUILD` side effect with only `BYPRODUCTS`.

## Scope / repro caveats

Observed via `just zephyr build-fixtures` on the **4.4 host-gcc** line
(`NROS_ZEPHYR_VERSION=4.4`, `ZEPHYR_TOOLCHAIN_VARIANT=host`) while validating
host-runnable zephyr (see [[0087-zephyr-3.7-native-sim-requires-sdk]] /
[[0086-zephyr-fixture-rustup-target-race]]). The ordering bug is generic to the
`nros-c` in-tree mirror, not zephyr-specific; CI's SDK path or different build
orders may mask it. The dep-edge fix is the durable resolution.

## Impact

Blocks the Zephyr **C** example fixtures (the Rust fixtures are unaffected — they
don't consume `nros_config_generated.h`). Orthogonal to phase-258; surfaced while
running test-all for it on a host without the pre-baked Zephyr SDK.
