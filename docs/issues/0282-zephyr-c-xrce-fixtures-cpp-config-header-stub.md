---
id: 282
title: "Zephyr XRCE fixtures raced the nros-cpp config header: unset NANO_ROS_PLATFORM disabled the ordering guard (0088/0090 class)"
status: open
type: bug
severity: medium
area: zephyr
related: [0088, 0090]
---

## Finding (2026-07-26, phase-305 lane sweep)

Four Zephyr fixtures fail to build:

- `build-c-talker-xrce`
- `build-c-service-server-xrce`
- `build-c-service-client-xrce`
- `build-c-action-server-xrce`

Each dies compiling the component's C TU:

```
packages/core/nros-cpp/include/nros/nros_cpp_config_generated.h:32:2:
  error: #error "nros_cpp_config_generated.h must be supplied per-build by
  the build system; see the comment in this stub for guidance."
```

i.e. the in-tree `#error` stub was reached instead of the per-build header
in `<build>/nros-rust/nros-cpp-generated/` — the 0088 / 0090 race class,
here on the Zephyr **C + XRCE** path only.

Scope is narrow and reproducible: every C+zenoh, C+cyclonedds and ALL C++
Zephyr fixtures build clean in the same sweep; only the four XRCE C
fixtures fail. The include ORDER is correct (the generated dir precedes the
stub dir on the command line), so this is an ordering/race problem — the
header does not exist yet when the TU compiles — not a search-path problem.

## Not caused by the RFC-0057 migration

Verified by two independent A/B runs on a clean disk (the earlier sweep's
noise was host ENOSPC, since resolved):

1. Reverting `examples/zephyr/c/talker/CMakeLists.txt` to the pre-phase-305
   fused `nano_ros_add_node(...)` spelling → same failure.
2. Additionally checking out `cmake/` from the commit before phase-305 W1
   → same failure.

So it predates phase-305; the migration only made the sweep run far enough
to surface it.

## Direction

Mirror the 0088/0090 remedy on the XRCE C path: ensure the component's TUs
carry a hard file-level `OBJECT_DEPENDS` on
`<build>/nros-rust/nros-cpp-generated/nros/nros_cpp_config_generated.h`
(phase-305 made `nano_ros_auto_add_library` APPEND rather than overwrite
that property, which is necessary but evidently not sufficient here), or
find why the XRCE Kconfig path pulls the nros-cpp header into a C TU at all
— the zenoh/cyclonedds C lanes do not.


## Correction + split (2026-07-26)

The original scope in this issue was WRONG, and the correction matters:

- I reported "only the 4 C+XRCE fixtures fail, the rest pass". They did not
  pass — `make -j4` **aborts after the first failures**, so every fixture
  scheduled behind them (including all C++/XRCE) never ran. "Not reported"
  was mistaken for "green".
- The real defect underneath is broader: **the Zephyr XRCE builds never
  produce `nros_cpp_config_generated.h` at all** — `<build>/nros-rust/`
  does not exist there, while the zenoh/cyclonedds peers carry the real
  header.

That splits cleanly by language:

### (a) C components — FIXED

`nros-c/component.h` probes for the generated sizes with `__has_include`,
intending it OPTIONAL (it falls back to static sizes). But `__has_include`
also matches the in-tree `#error` stub, which is always on the include
path, so the optional probe detonated on any build that never produces the
real header. Fixed: probes announce themselves
(`NROS_CPP_CONFIG_OPTIONAL`) and the stub stays silent **for C probes
only**, without defining the include guard (so a later mandatory include
still reaches the hard error). All 5 Zephyr C+XRCE fixtures now build.

### (b) C++ components — OPEN, the real defect

`nros-cpp` headers (`publisher.hpp`, `client.hpp`, … via `config.hpp`)
consume `NROS_*_SIZE` and have NO static fallback — they cannot work
without the generated header. On XRCE the header is never produced, so
every C++/XRCE fixture fails (currently as a cascade of
`'storage_' was not declared`).

Fix direction: make the Zephyr XRCE build produce/mirror
`nros_cpp_config_generated.h` like the zenoh and cyclonedds lanes do —
i.e. find why the nros-cpp cargo build + header mirror is skipped for
`NROS_RMW=xrce`. Papering this over on the include side would only move
the failure later.


## Second correction — the actual root cause (2026-07-26)

The "(b) XRCE never produces the header" conclusion above was ALSO wrong.
Direct evidence from a manual `west build` of `examples/zephyr/cpp/talker`
with `CONF_FILE="prj.conf;prj-xrce.conf"`:

- `CONFIG_NROS_CPP_API=y`, `CONFIG_NROS_RMW_XRCE=y`
- `<build>/nros-rust/nros-cpp-generated/nros/nros_cpp_config_generated.h`
  EXISTS, in exactly the same layout as the zenoh peer

So the header is produced correctly. This is the **0088/0090 compile
race**, and the reason it bit XRCE deterministically:

`nano_ros_auto_add_library` (and the legacy fused register path) gated
their Zephyr ordering logic on `NANO_ROS_PLATFORM STREQUAL "zephyr"` —
but that variable is **not set in a `find_package(Zephyr)` app build**.
The guard silently never fired, so the component library received no
ordering edge to the nros-cpp cargo build. Proof from the generated
`build.ninja`:

```
build cmake_object_order_depends_target_talker_lib: phony || \
    zephyr/driver_validation_h_target zephyr/kobj_types_h_target \
    zephyr/syscall_list_h_target
build CMakeFiles/talker_lib.dir/src/Talker.cpp.obj: … | \
    nros-rust/nros-c-generated/nros/nros_config_generated.h \
    nros-rust/nros-c-generated/nros/nros_generated.h || …
```

— only Zephyr's own header targets in the order phony, and explicit deps
on the nros-**c** generated headers but never nros-**cpp**. zenoh and
cyclonedds won that race on timing; XRCE lost it every time.

Fix: detect Zephyr by the reliable signal (`TARGET app` + `ZEPHYR_BASE`)
instead of the unset variable, and add BOTH a target-level dependency on
the cargo-build targets and the file-level `OBJECT_DEPENDS`. Verified:
`examples/zephyr/cpp/talker` + XRCE builds clean where it previously
failed.

Note the same latent guard bug exists on the legacy fused
`nano_ros_node_register` Zephyr branch — zenoh/cyclonedds only pass there
by timing luck.
