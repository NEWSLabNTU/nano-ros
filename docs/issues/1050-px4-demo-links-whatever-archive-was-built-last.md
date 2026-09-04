---
id: 1050
title: "`just px4 build-sitl-example` links whatever `libnros_cpp.a` was built last, so a uORB-only module fails at `nros::init()` because a backend it never declared won slot 0"
status: open
type: bug
area: build, testing
severity: high
found: 2026-09-04
related: [1046, 0436, phase-325, 0616]
---

# Same source, same recipe, same command — different result

## Reproduced, by changing exactly one variable

`nros_uorb_demo start` on a correctly-built SITL tree:

    ERROR [nros_uorb_demo] nros::init() failed
    ERROR [nros_uorb_demo] Task start failed (-1)

`task_spawn` returns `PX4_ERROR` when `init()` fails, and the PX4 shell reports
that as status **255** — which is what `px4_uorb_interop_e2e.rs` dies with.

Rebuild `libnros_cpp.a` without zenoh, rebuild the module, change nothing else:

    $ ./bin/px4-nros_uorb_demo start      # (silent — success)
    $ ./bin/px4-nros_uorb_demo status
    INFO  [nros_uorb_demo] published: 0  received: 0

Both `nros_rmw_uorb_register_topic` calls succeed in both cases; the failure is
the session open.

## The mechanism

`just px4 build-sitl-example` (`just/px4.just`) is four lines: it checks for the
PX4 tree and runs `make px4_sitl_default`. **It does not build `libnros_cpp.a`,
and does not check which one is there.** It links against whatever the last
`cargo build -p nros-cpp` left in `target/release/`.

The module's own generated stub registers exactly what it declared:

```c
/* Backends registered: uorb */
extern int nros_rmw_uorb_register(void);
void nros_app_register_backends(void) { (void)nros_rmw_uorb_register(); }
```

But a zenoh-enabled archive **also** registers zenoh, through the `.init_array`
constructor that survives on hosted Rust backends
(`packages/rmw/zenoh/nros-rmw-zenoh/src/lib.rs`). PX4 SITL is hosted POSIX, so
that ctor runs **before `main`** — therefore before `nros_app_register_backends()`
is ever called. zenoh lands in slot 0.

`nros::init()` opens the DEFAULT slot. It gets zenoh, dials the default
`tcp/127.0.0.1:7447`, finds no router (the one running on this host was on
`:32823`), and fails.

**A uORB-only module, broken by a backend it never declared, winning a race it
should not have been in.**

## The coupling is circular, which is why it also breaks the LINK

The generated header the module compiles against
(`target/nros-cpp-generated/nros_cpp_config_generated.h`) comes from that same
ambient archive, so the module's expected feature VARIANT is decided by whatever
someone built previously. Building the bridge and then the demo produces:

    undefined reference to `nros_cpp_config_variant_alloc_env_platform_posix_
        rmw_cffi_rmw_zenoh_cffi_ros_humble_std'

i.e. the demo compiled expecting an archive it is not being linked against. The
anchor is doing its job; the recipe is what put it in that position.

## FIXED 2026-09-04 (defect 1 only) — proven from the broken state

`build-sitl-example` now builds the archive it links, with `rmw-cffi` (uORB is
this module's outward side), the way `build-bridge-example` always has for the
opposite reason.

Verified by starting from the FAILING condition rather than a good tree — the
weaker direction would have passed before the fix too, which is how issue 1046's
guard survived:

    archive poisoned with rmw-zenoh-cffi   -> 3 nros_rmw_zenoh_register
    just px4 build-sitl-example            -> archive rebuilt, 0 zenoh_register
    ./bin/px4-nros_uorb_demo start         -> silent (success; was status 255)
    ./bin/px4-nros_uorb_demo status        -> published: 0  received: 0

## Three separable defects

1. **A recipe consumes a shared artifact it neither builds nor verifies.**
   `build-bridge-example` builds its own archive (stage [3/4]) precisely because
   the outward backend is chosen there; `build-sitl-example` never got the same
   treatment. This is issue 1046's class one layer up — an artifact standing in
   for a fact, outliving the fact.
2. **`.init_array` registration races the explicit stub.** `BACKENDS uorb` reads
   as "this image registers uorb", and on a hosted target it does not mean that:
   any backend compiled into the archive registers itself first. The keyword's
   own docs in `NanoRosPx4Module.cmake` describe the generated stub as the
   registration mechanism, which is true on RTOS targets and incomplete on POSIX.
3. **`nros::init()` takes slot 0**, so the race above silently decides the
   transport. A module that wants uORB has no way to say so through `nros::init()`
   — `Executor::open_with_rmw("uorb", …)` exists, but the C++ one-liner the
   examples use does not reach it.

Only (1) is fixed here. (2) and (3) are design questions: (2) would mean either
suppressing the ctor path when a stub is generated, or making `BACKENDS` an
assertion rather than a hint; (3) would mean the examples naming their backend.
Both change behaviour for every consumer, so they want an owner and a decision,
not a drive-by.

## Not covered

* Whether `build-sitl-cpp` (the register-check gate) has the same exposure. Its
  comments quote the `cargo build -p nros-cpp --features std,rmw-cffi` command,
  but whether the recipe RUNS it was not checked.
* Whether any non-PX4 lane links `libnros_cpp.a` ambiently the same way.
* Whether the 1046 guard should also assert the archive's feature variant. It
  currently answers "is this module linked", not "was it linked against the
  right archive" — a strictly harder question, and arguably the anchor symbol
  already answers it at link time.
