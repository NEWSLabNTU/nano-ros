---
id: 1050
title: "`just px4 build-sitl-example` links whatever `libnros_cpp.a` was built last, so a uORB-only module fails at `nros::init()` because a backend it never declared won slot 0"
status: open
type: bug
area: build, testing
severity: high
found: 2026-09-04
# defects (1) and (2) are fixed; (3) — `nros::init()` takes slot 0 — is why
# this stays open.
related: [1046, 0436, phase-325, 0616, phase-424]
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

## (2) FIXED 2026-09-05 — `BACKENDS` is an assertion now, taking the cheaper of
## the two options the paragraph below offered

The choice was "suppress the ctor path when a stub is generated" or "make
`BACKENDS` an assertion rather than a hint". The first changes runtime behaviour
for every consumer; the second is a configure-time check. Took the second.

* Whether `build-sitl-cpp` (the register-check gate) has the same exposure. Its
  comments quote the `cargo build -p nros-cpp --features std,rmw-cffi` command,
  but whether the recipe RUNS it was not checked.
* Whether any non-PX4 lane links `libnros_cpp.a` ambiently the same way.
* Whether the 1046 guard should also assert the archive's feature variant. It
  currently answers "is this module linked", not "was it linked against the
  right archive" — a strictly harder question, and arguably the anchor symbol
  already answers it at link time.

## Defect 1's other half — 2026-09-04, phase-424

The recipe fix above makes `build-sitl-example` build the archive it links. That
closes the DEFAULT path and cannot close the general one, because the archive
path is overridable (`NROS_CPP_ARCHIVE` / `-DNROS_CPP_ARCHIVE=`) and the
generated-header path is not — it is derived from `NANO_ROS_ROOT`. An override
therefore un-pairs them by construction, and no recipe can speak for that.

So the pairing is now asserted where the link is decided, in
`integrations/px4/NanoRosArchivePairing.cmake`: the variant symbol is read out of
the header and the archive must define it. This is the same condition the issue
0360 anchor enforces at LINK time — the `undefined reference to
nros_cpp_config_variant_..._rmw_zenoh_cffi_...` recorded above — evaluated at
CONFIGURE time instead, where the message can name the cargo command. That trade
is already `_nros_px4_resolve_archive`'s stated policy; this extends it from
"does the archive exist" to "is it the right one". Detail, the `nm` trap, and the
mutation checks are in issue 1046.

**Reproduced against the fix, from the failing state.** Two real cargo
invocations, paired crosswise:

    cargo build -p nros-cpp --features std,rmw-cffi,platform-posix   -> A
        variant nros_cpp_config_variant_alloc_env_platform_posix_rmw_cffi_std
    cargo build -p nros-cpp --features std,rmw-cffi                  -> B
        variant nros_cpp_config_variant_alloc_env_rmw_cffi_std

    header B + archive B  -> PASS
    header A + archive B  -> FAIL: "the generated header and the archive are
                                    from DIFFERENT builds", naming both variants
    header A + archive A  -> PASS

## The three "not covered" questions, answered

1. **Does `build-sitl-cpp` have the same exposure? No — measured.** It points
   `EXTERNAL_MODULES_LOCATION` at `packages/testing/nros-px4-register-check`,
   which calls raw `px4_add_module()`, not `nros_px4_add_module()`, and grep finds
   no reference to `libnros_cpp`, `nros_px4_add_module` or `NanoRosPx4Module` in
   that tree at all. It compiles the uORB backend's C++ sources INLINE and links
   `src/register_fallback.c`'s weak `nros_rmw_cffi_register`. It consumes no
   nano-ros archive, so there is nothing for it to be ambient about. Its comments
   quoting the cargo command are stale prose, not a dependency.

2. **Does any non-PX4 lane link `libnros_cpp.a` ambiently? Not swept, and not
   claimed.** Every user of `nros_px4_add_module` is enumerated — the bridge, the
   demo, and `integrations/px4/module-template` — and all three now go through the
   pairing check. The other consumers of that archive (zephyr, nuttx, threadx,
   the cmake entry path) reach it through Corrosion or an explicit build rule
   rather than a bare path, which is a different mechanism; nobody checked whether
   any of them can un-pair a generated header the same way.

3. **Should the guard also assert the archive's feature VARIANT? It now does, and
   that is a strictly stronger question than the one issue 1046 answered.**
   1046's guard answers "is this module linked into `bin/px4`"; the pairing
   answers "was it compiled against the archive it is linked to". They are
   different failures at different layers — 1046's fires after a PX4 build
   succeeded with the wrong root, this one before a PX4 build starts — and
   neither subsumes the other. The suggestion that the anchor already answers it
   is right about the FACT and wrong about the COST: the anchor answers it ~1100
   targets into a ten-minute build, in a message that names a mangled variant and
   no command.

Defects **(2)** `.init_array` racing the explicit stub and **(3)** `nros::init()`
taking slot 0 are untouched. Both change behaviour for every consumer and still
want an owner.

## Not verified

**No PX4 build was run.** `third-party/px4/PX4-Autopilot` is an empty,
uninitialised gitlink on this host (`git submodule status` reports the leading
`-`), so nothing here passed through a real `make px4_sitl_default`. What is
measured is the cmake predicate against real `libnros_cpp.a` artifacts and real
generated headers; what is inferred is that it fires in the same place during a
PX4 configure, which follows from it sitting at file scope in a module every PX4
root includes, but was not observed.

## Status: STAYS OPEN, narrowed

Defect **(1)** is closed in both halves — the recipe builds the archive it links,
and the configure asserts the pair agrees even when the path is overridden.
Defects **(2)** and **(3)** are untouched, and no other issue covers them, so
this one remains their home rather than being resolved out from under them.
Deliberately not spun out into a new issue: neither is a bug with a fix waiting,
they are decisions about what `BACKENDS` and `nros::init()` mean on a hosted
target, and splitting them would scatter the reasoning that makes them one
question.

**The precheck already existed and was blind in exactly the direction that
matters.** `nros_px4_add_module` ran its `llvm-nm` check inside
`if(_networked_backends)` — the set of modules this bug CANNOT happen to — and
inside it only asked "is every DECLARED backend present". The uORB demo declares
`BACKENDS uorb`, so the whole block was skipped for it. That is issue 1046's
shape one layer up: a guard whose predicate cannot observe the case its own
message describes.

So the check now runs for EVERY module and asks both directions:

* (a) every declared networked backend is in the archive (unchanged);
* (b) **no undeclared one is**, because on hosted POSIX it registers itself
      first and takes slot 0.

Measured, through the real `nros_px4_add_module` with a stub `px4_add_module`:

| module `BACKENDS` | archive | before | after |
| --- | --- | --- | --- |
| `uorb` | zenoh-carrying (the bug) | configure OK, dies at `start` | **FATAL, names the rebuild** |
| `uorb` | `rmw-cffi` only | OK | OK |
| `uorb zenoh` | `rmw-cffi` only | FATAL (missing) | FATAL (missing) |
| `uorb zenoh` | zenoh-carrying (bridge) | OK | OK |

The discriminator is the unmangled C symbol `nros_rmw_<b>_register`, matched on
word boundaries because the same archive carries Rust-mangled names that contain
its prefix (`_RNvNtCs..._14nros_rmw_zenoh13cffi_register8register`) — a bare
substring test would read those as a definition. One predicate serves both
directions, so presence and absence cannot drift apart.

**Cost, against phase-424's constraint:** one extra `llvm-nm --defined-only` per
configure for a uORB-only module, **measured 0.012 s** on the 26 MB archive. It
hashes nothing and watches nothing, so it widens no watch set and re-stales
nothing — issue 0835's budget is untouched by construction, not by measurement.

This is also what makes the (1) fix hold at the SEAM rather than only in the
recipe. `just px4 build-sitl-example` builds the archive it links, which is
right, but a hand-run `make px4_sitl_default EXTERNAL_MODULES_LOCATION=...`
against an archive some other recipe left behind bypasses it entirely — and
that state is present on this host right now: the ambient
`target/release/libnros_cpp.a` defines `nros_rmw_zenoh_register`.

## (3) is the one still open

`nros::init()` takes slot 0, so *which* backend a module gets is still decided
by registration order rather than by the module. (2)'s fix removes the way that
order goes wrong by accident; it does not give a C++ one-liner any way to NAME
its backend. `Executor::open_with_rmw("uorb", …)` exists and the examples'
`nros::init()` does not reach it. That is a consumer-facing API decision and
still wants an owner.

## Three separable defects (original framing, kept)

Only (1) was fixed at filing. (2) is fixed above. (3) remains.

## Not covered — swept 2026-09-05

* **`build-sitl-cpp` has NO exposure.** Its root,
  `packages/testing/nros-px4-register-check`, calls `px4_add_module` directly —
  not `nros_px4_add_module` — and links no nano-ros archive: it compiles the
  uORB backend's sources inline and its only Rust seam is the weak
  `register_fallback.c`. So there is no ambient archive for it to inherit. (The
  `cargo build -p nros-cpp --features std,rmw-cffi` command this issue thought
  was quoted in `build-sitl-cpp`'s comments is in fact quoted in
  `build-sitl-example`'s prereq block and in `NanoRosPx4Module.cmake`'s header —
  that claim in the original text was wrong.)
* **No non-PX4 lane links `libnros_cpp.a` ambiently.**
  `${NANO_ROS_ROOT}/target/release/libnros_cpp.a` as a path is named in exactly
  one place in the tree, `integrations/px4/NanoRosPx4Module.cmake:86`. Every
  other consumer reaches the umbrella through Corrosion/cmake, which builds it
  per configure.
* **The 1046 guard should NOT also assert the feature variant.** The anchor
  symbol `nros_cpp_config_variant_...` already makes a header/archive mismatch a
  LINK error, which this issue's own repro shows firing; and the archive check
  above now catches the backend half at CONFIGURE time, which is earlier than
  either. A third copy in the test would be a fourth spelling of the same fact.

## NOT the issue-0475 class — checked

`libnros_cpp.a` is not reached through a raw `-Wl,` flag: the helper passes it
to `target_link_libraries(${NPX_MODULE} PUBLIC <absolute path>)`, and CMake does
create a file-level edge for an absolute-path link item. Confirmed in the real
build graph — it is under `|` (implicit), not `||` (order-only):

```
$ ninja -C build/px4_sitl_default -t query bin/px4 | grep -i nros
    | external_modules/modules/nros_uorb_bridge/libmodules__nros_uorb_bridge.a
    | .../target/release/libnros_cpp.a
    | .../build/nros-platform-posix/libnros_platform_posix.a
    | .../examples/px4/cpp/bridge/ffi/target/release/libnros_px4_bridge_ffi.a
```

So `bin/px4` DOES relink when the archive changes. The defect was never a
missing edge — it was that nothing decided WHICH archive should be there.
