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

## Fix direction

- Confirm whether a from-scratch `build-fixture-extras` (wipe `build-cyclonedds`
  first) produces a listener that registers. If yes → the register -1 is specific
  to the targeted `native-cyclonedds-cmake` invocation missing a descriptor-
  registration step (or a stale prebuilt descriptor TU); make that path regenerate
  or fail loud on a missing std_msgs Cyclone descriptor.
- If a from-scratch extras build ALSO fails register -1 → a real regression in the
  native cyclone reader-creation path (nros-c executor / nros-rmw-cyclonedds) vs
  the old prebuilt; bisect against the last-known-good cyclone C listener.

## References

`examples/native/c/listener/src/main.c:150`,
`packages/core/nros-c/src/executor.rs::nros_executor_register_subscription`,
`packages/dds/nros-rmw-cyclonedds/src/descriptors.cpp` (registration surface),
`just/native.just::build-fixture-extras` (native-cyclonedds-cmake cell), issue #183
(surfaced it), issue #175 (Cyclone descriptor registration).
