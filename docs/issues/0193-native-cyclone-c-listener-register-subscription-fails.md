---
id: 193
title: "Freshly-built native cyclone C listener fails nros_executor_register_subscription -> -1 at startup"
status: open
type: bug
area: build
related: [issue-0183, issue-0181, issue-0175]
---

## Summary

A native cyclone C listener (`examples/native/c/listener`, `build-cyclonedds/c_listener`)
rebuilt against a **freshly-provisioned** cyclonedds (`third-party/dds/cyclonedds`,
0.10.5-nros1, via `just cyclonedds setup`) boots, creates its subscription, then
fails to register it with the executor:

```
nros C Listener
Locator: tcp/127.0.0.1:7447
Domain ID: 88
Support initialized
Node created: listener
Subscription created for topic: /chatter
[nros] .../examples/native/c/listener/src/main.c:150
  nros_executor_register_subscription(&app.executor, &app.subscription,
  NROS_EXECUTOR_ON_NEW_DATA) -> -1
```

`nros_subscription_init` succeeds ("Subscription created"), but
`nros_executor_register_subscription` returns `-1` — the executor's reader
creation fails, so no callback ever fires and the listener receives nothing.

## Scope — it is NOT the message type

Reproduced on the **String default** AND with `NROS_SUB_TYPE=int32` (issue #183's
env-select). The String path is the unmodified stock listener, so this is not the
#183 Int32 change — it is the build. The **previously-prebuilt** listener (from an
earlier cyclonedds provisioning) registered fine and delivered samples end-to-end,
so a fresh cyclone C listener build regressed relative to the old artifact.

## Two build paths, two failure modes

- `fixture-make-driver.sh native-cyclonedds-cmake` (the fixture recipe's cyclone
  cell, `-DNANO_ROS_BUILD_CODEGEN=OFF`): **builds**, but the listener hits the
  `register_subscription -> -1` above at runtime. Likely the std_msgs Cyclone
  `dds_topic_descriptor_t` is not registered with the participant (CODEGEN=OFF
  consumes a prebuilt/static descriptor-registration TU that is stale or absent
  after the fresh provisioning), so `dds_create_reader` fails. Same descriptor-
  registration surface as #175.
- `just native _build-c-example examples/native/c/listener "-DNANO_ROS_RMW=cyclonedds"`
  (CODEGEN=ON): **fails at cmake configure** in `nros_generate_interfaces`
  (`NanoRosCodegenCore.cmake:402`), so it never links; also targets `build/`, not
  the `build-cyclonedds/` the tests read.

## Why it matters

Blocks every native cyclone C listener e2e (and the #183 declarative ws-bridge
final verification) whenever the fixtures are rebuilt against a fresh cyclonedds.
A clean `just native build-fixture-extras` is the presumed correct path, but it is
a ~3 h cold-cyclonedds rebuild here and incremental-skips an already-built (broken)
`build-cyclonedds/c_listener`, so it did not exercise a from-scratch listener in
this session. This is #181-class (fixture-build robustness / silent lane gap) but
manifests at RUNTIME (register -1), not as a missing binary.

## ROOT CAUSE CONFIRMED — CMake < 3.24 defeats the descriptor whole-archive — 2026-07-13

The register -1 is `find_descriptor(std_msgs::msg::dds_::String_) -> nullptr` in
`subscriber.cpp::subscriber_create`: NO cyclone descriptor is registered because the
per-type `register_std_msgs_<Type>_0` `__attribute__((constructor))` TUs are GC'd
out of the executable. Confirmed:

- `libstd_msgs__cyclonedds_ts.a` **contains** the ctors (`nm` shows
  `register_std_msgs_String_0_constructor` etc.).
- `c_listener` (the exe) contains **ZERO** of them (`nm | grep -c … == 0`).

The force-load lives in `cmake/NanoRosGenerateInterfaces.cmake` (~line 720): the
descriptor ts static lib is whole-archived into the interface lib so its static-init
ctors survive `--gc-sections`. That path branches on CMake version:

- **CMake ≥ 3.24** → `$<LINK_LIBRARY:WHOLE_ARCHIVE,${target}__cyclonedds_ts>` — a
  de-dup-AWARE generator expression. Works (the previously-prebuilt listener, built
  with ≥3.24, registered fine).
- **CMake < 3.24** (this host is **3.22.1**, the floor `#181`/`08906f616` set) →
  the `else()` fallback `-Wl,--whole-archive ${target}__cyclonedds_ts
  -Wl,--no-whole-archive`. CMake **de-dupes the target NAME out of the group**, so
  the ctors GC. Attempting to repair it with `$<TARGET_FILE:…>` (raw path, in the
  group) does NOT work either: CMake auto-adds the ts lib target as a **competing
  PLAIN link item** that de-dupes against the whole-archived path — and it keeps the
  PLAIN (GC-able) copy. Verified on a clean from-scratch rebuild:
  `libstd_msgs__cyclonedds_ts.a` lands plain on the link line, ctors still absent.
  (A dependency's ts lib, e.g. `builtin_interfaces`, has no competing plain link so
  it DOES survive — which is why only the target's own descriptor is lost.)

So the register -1 is deterministic on any cyclone C example built with CMake < 3.24.

## Fix direction

The clean fix is the 3.24 de-dup-aware `$<LINK_LIBRARY:WHOLE_ARCHIVE>` semantics —
so **require / provide CMake ≥ 3.24 for the cyclone fixture cells** (gate the
`native-cyclonedds-cmake` build on it and fail loud if `cmake --version` < 3.24,
rather than silently GC-ing descriptors → register -1). Bumping the effective build
CMake (e.g. a pip `cmake` ≥3.24 on PATH) is the least-effort unblock; the `#181`
minimum floor of 3.22 can stay for non-cyclone lanes.

A pure 3.22 code fix is fragile: the manual `--whole-archive` is inherently
de-dup-unsafe. If a 3.22 fix is mandatory, the robust options are (a) eliminate the
competing plain ts-lib link item so only the whole-archived path remains, or
(b) emit explicit `-u register_<pkg>_<Type>_0` undefined-symbol refs per generated
type (a `configure_file`-time enumeration). Both are more involved and were not
landed here.

## References

`examples/native/c/listener/src/main.c:150`,
`packages/core/nros-c/src/executor.rs::nros_executor_register_subscription`,
`packages/dds/nros-rmw-cyclonedds/src/descriptors.cpp` (registration surface),
`just/native.just::build-fixture-extras` (native-cyclonedds-cmake cell), issue #183
(surfaced it), issue #175 (Cyclone descriptor registration).
