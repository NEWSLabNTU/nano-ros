---
id: 166
title: "Zephyr zenoh e2e tests serialize on build-time-baked router ports — a runtime-locator override would unlock per-test parallelism"
status: open
type: enhancement
area: testing
related: [phase-89, issue-0141, phase-286]
---

## Problem

The `tests/zephyr.rs` family still serializes its slowest lanes. A full run is
~**292 s**, and the tail is the zenoh DDS runtime e2e tests, which nextest pins
to `max-threads = 1` across **six** groups:

```
[test-groups.qemu-zephyr-pubsub-rust]   max-threads = 1
[test-groups.qemu-zephyr-pubsub-cpp]    max-threads = 1
[test-groups.qemu-zephyr-service-rust]  max-threads = 1
[test-groups.qemu-zephyr-service-cpp]   max-threads = 1
[test-groups.qemu-zephyr-action-rust]   max-threads = 1
[test-groups.qemu-zephyr-action-cpp]    max-threads = 1
```

This is NOT because the port scheme is coarse. `nros_tests::platform::ZEPHYR`
already assigns unique per-(variant, lang) host ports
(`zenohd_port = 7456`, `lang_stride = 100`; `xrce_agent_port = 2018`,
`xrce_lang_stride = 100`), and the sibling groups exploit it well:
`qemu-zephyr-xrce = 7`, `qemu-zephyr-dds = 4`, `zephyr-native-cyclonedds`
parallel-by-domain, fall-through `qemu-zephyr = 6`.

The blocker is granularity **below** (variant, lang): the `zephyr` binary has
**multiple tests per (variant, lang)** — e.g. `test_zephyr_talker_to_listener_e2e`,
`test_zephyr_to_native_e2e`, `test_zephyr_server_native_client`,
`test_bidirectional_native_zephyr_e2e` all map to the pubsub/service-rust slot —
and they all reuse the SAME fixture image, whose router port is **baked at build
time**:

```sh
# scripts/build/zephyr-fixture-leaves.sh
zenoh_locator="tcp/127.0.0.1:$zenoh_port"
extra_cmake_defs="$extra_cmake_defs -DCONFIG_NROS_ZENOH_LOCATOR=\"$zenoh_locator\""
```

One baked port per image ⇒ every test that dials that image shares one zenohd on
one port ⇒ they must run serial or their routers collide (the #141 class).

> **Design update (2026-07-09, phase-286 W1).** The "runtime `NROS_LOCATOR` (env)"
> phrasing below resolves concretely to a **native command-line option**
> (`--nros-locator=<loc>`), NOT `getenv`: `nsi_host_getenv` is absent from this
> Zephyr 3.7 LTS tree and the embedded images are `no_std`, but native_sim already
> takes CLI args (the `--seed` the harness passes, registered via
> `native_add_command_line_opts` + `NATIVE_TASK(PRE_BOOT_1)`). Full mechanism +
> read-site/XRCE edges → phase-286 W1 design findings.

## Opportunity — runtime locator override on native_sim

The whole `zephyr` family runs on **native_sim**, which is an ordinary host
process. It can read a runtime env var at startup, exactly like the native
fixtures do (`NROS_LOCATOR`), instead of consuming the compile-time
`option_env!("NROS_LOCATOR")` → `CONFIG_NROS_ZENOH_LOCATOR` bake. If the
native_sim path preferred a **runtime** `NROS_LOCATOR` (env) over the baked
default, each test could:

1. allocate an ephemeral free port (`TcpListener::bind("127.0.0.1:0")`),
2. start its own zenohd on it,
3. pass `NROS_LOCATOR=tcp/127.0.0.1:<port>` to both fixture processes,

giving every test invocation a unique port with zero static coordination — which
retires all six `max-threads = 1` groups and lets the DDS-zenoh e2e lanes run at
the same width as the xrce/dds groups.

The baked value stays the default, so real QEMU / hardware images (which cannot
reach a host env and MUST bake) are unaffected — the override is a native_sim
fast path, and native_sim is exactly where the serial cost lives.

## Estimated win

The six serial groups hold the heaviest tests (each boots two native_sim images
+ a zenohd, ~2–3 s apiece), ~15–20 tests run strictly one-at-a-time today.
Parallelizing them to the host core count should cut several minutes off a full
`--test zephyr` run and off the nightly Zephyr CI lane.

## Alternatives considered

- **Finer baked-port granularity (per test-scenario, not per (variant, lang)).**
  Rejected: tests share fixture *images* (one talker image, many tests), so
  per-test ports would require per-test fixture copies — a fixture-count
  explosion for no gain the runtime override doesn't already give.
- **Leave serial.** The current split already parallelizes xrce/dds; this is the
  last serial island, and it is the largest single contributor to wall-clock.

## References

`packages/testing/nros-tests/src/platform.rs` (the port scheme),
`.config/nextest.toml` (the six serial groups + the parallel xrce/dds ones),
`scripts/build/zephyr-fixture-leaves.sh` (the `CONFIG_NROS_ZENOH_LOCATOR` bake),
`examples/zephyr/rust/talker/build.rs` (`option_env!` → bake), issue #141 (the
router-port-collision hazard the serialization currently prevents).
