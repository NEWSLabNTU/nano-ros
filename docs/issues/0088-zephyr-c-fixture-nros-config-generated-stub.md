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

## Likely cause

The Zephyr C example build resolves `<nros/nros_config_generated.h>` to the
in-tree **stub** dir, and either (a) the mirror custom command hasn't run before
the app TU compiles (missing target ordering / dependency edge in the Zephyr
`add_subdirectory` flow), or (b) the Zephyr build-dir layout puts the corrosion
build dir's generated header somewhere not on the app's include path. The cargo
build of `nros-c` succeeds (build.rs runs), so the header IS produced — it just
isn't on the C app's include path at TU-compile time.

## Scope / repro caveats

Observed via `just zephyr build-fixtures` on the **4.4 host-gcc** line
(`NROS_ZEPHYR_VERSION=4.4`, `ZEPHYR_TOOLCHAIN_VARIANT=host`) while validating
host-runnable zephyr (see [[0087-zephyr-3.7-native-sim-requires-sdk]] /
[[0086-zephyr-fixture-rustup-target-race]]). Not yet confirmed whether the
SDK-provisioned CI path (`/opt/zephyr-sdk`, dual 3.7+4.4 in `nightly.yml`) hits
the same ordering — CI may sequence the mirror differently or the Rust fixtures
(which DON'T include the C header) mask it. Confirm on CI + on 3.7 before fixing.

## Impact

Blocks the Zephyr **C** example fixtures (the Rust fixtures are unaffected — they
don't consume `nros_config_generated.h`). Orthogonal to phase-258; surfaced while
running test-all for it on a host without the pre-baked Zephyr SDK.
