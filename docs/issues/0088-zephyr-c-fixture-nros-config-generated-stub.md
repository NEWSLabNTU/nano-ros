---
id: 88
title: Zephyr C fixture compiles against the nros_config_generated.h stub (per-build header not on the C app include path)
status: open
type: bug
area: zephyr
related: [phase-258, 0086, 0087]
---

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
