---
id: 408
title: Seven native e2e tests fail on main — per-node projection (params, remap,
  qos, component order) does not reach the wire
status: open
type: bug
area: orchestration
related: [0398, 0382, phase-330, rfc-0066, 0401]
---

## Problem

Seven tests fail on `main` (`55a2df1e0`), reproducibly, **serially, against
fixtures rebuilt from that exact tree**. They are not flakes and not stale
fixtures — both of those were ruled out by re-running (see Evidence).

| Test | Message |
| --- | --- |
| `cpp_c_param_live_read_e2e::c_param_live_read_publishes_baked_initial` | C component never published live-read baked param (250) on `/chatter` — `nros_cpp_get_param_integer` did not reach the callback |
| `param_live_read_e2e::param_live_read_publishes_resolved_value` | subscriber saw no `/chatter` value at all — the live param read never reached the wire |
| `workspace_features_e2e::case_01_native_c_custom_msg` | `reading_listener` never received 3 custom-msg samples |
| `workspace_features_e2e::case_08_native_c_qos` | `qos_listener` never received 3 QoS-matched samples |
| `workspace_features_e2e::case_17_native_rust_remap` | `/remapped_out` never received 3 samples |
| `declarative_bridge_zenoh_to_cyclonedds::declarative_zenoh_to_cyclonedds_nested_header_to_ros2` | stock ros2 cyclone subscriber received no bridged Header on `/header` |
| `nano2nano::test_tls_talker_listener_communication` | Listener: expected at least 1 received messages, got 0 |

## Already filed separately: component order

`cpp_multi_node_entry::multi_node_workspace_cpp_typed_configures_and_builds`
(`component order doesn't match launch XML`) was in the original list of eight
and is **#0382**, open since 2026-08-01 with the root cause already identified:
the resolver serializes `structure.nodes` alphabetically, so launch declaration
order is lost and the entry emitter iterates file order. Not re-filed here.

That leaves SEVEN failures below.

## Why these look like ONE fault, not seven

Five of the seven are per-node projections of a launch/model: component ORDER,
per-node PARAMS (both the C and the Rust path), REMAP, and QoS. That is exactly
the surface #0398 describes — "`[[component]] name` no longer matches the launch
node name, so every per-node projection keyed on it silently does nothing" —
and exactly what phase-330 W7 has been moving (`launch -> model` mapping,
the cmake `LAUNCH` keyword, the Rust `model=` sweep).

The projection failures are all SILENT at build time: every fixture compiles and
runs, and the node simply never sees its parameter / remap / ordering. That is
the failure mode #0398 predicts.

The bridge and TLS cases fit less well and may be separate faults; they are
listed here because they came from the same run, not because the cause is known
to be shared.

## Root cause of the five workspace-feature failures — FIXED

A service server IS a zenoh queryable, and the C shim's queryable table is a
STATIC array of `ZPICO_MAX_QUERYABLES` slots. The ROS parameter services are 6
and the REP-2002 lifecycle services are 6, so an entry enabling both consumes
twelve before the application declares anything. The default was 8 on every
target. `examples/workspaces/features` declares
`features = ["param_services", "lifecycle"]` workspace-wide, so ALL FIVE of its
entries overflowed the table at boot:

    nros: application error: NodeRegister("lifecycle")

The tests reported "listener never received N samples", which reads as a
delivery fault. Nothing was ever published — the publisher had already exited.

Fixed: the default is now 32 on hosted targets and unchanged at 8 for
`target_os = "none"` (8 is an embedded RAM budget that had been applied to Linux
hosts), and the overflow logs the exhausted knob plus the 6+6 arithmetic instead
of returning a bare `ServiceServerCreationFailed` that `apply_lifecycle` then
flattened to `()`.

Verified: every entry goes from `NodeRegister("lifecycle")` to
`application complete`; `case_01_native_c_custom_msg` and `case_08_native_c_qos`
pass. Remaining open here: the zenoh->cyclonedds bridge and the TLS
talker/listener cases, which are separate and unexamined.

## TLS — FIXED

`nano2nano::test_tls_talker_listener_communication` failed with
"Listener: expected at least 1 received messages, got 0". The session never
opened. `strace` on the client:

    connect(::1:7922)       = ECONNREFUSED
    connect(127.0.0.1:7922) = EINVAL

The harness starts the TLS router on `tls/127.0.0.1` (IPv4 only) while clients
reach it by NAME — `tls/localhost:<port>`, required because the self-signed test
cert is CN=localhost. On a dual-stack host `localhost` resolves to `::1` FIRST,
that attempt is refused, and zenoh-pico then fails the IPv4 fallback with EINVAL
because it retries on the SAME socket instead of a fresh one. The plain-TCP
lanes never hit this: they use a literal 127.0.0.1.

Fixed by listening on `tls/[::]:<port>`, which accepts both families
(bindv6only=0). Verified: the test passes; `openssl s_client` confirmed the
server side was healthy all along (TLS 1.2 and 1.3, verify OK).

Worth a separate fork fix: the EINVAL retry is a zenoh-pico robustness bug — a
failed connect leaves the socket unusable and the next candidate address needs a
new one. A dual-stack host talking to an IPv4-only peer is not exotic.

## Bridge — FIXED (a metadata-precedence regression)

CORRECTION to the first pass below: `/header` IS declared. It is not in the
node's CODE, it is in `talker_pkg/Cargo.toml`:

```toml
[[package.metadata.nros.node.publishes]]
topic = "/chatter"
type = "std_msgs/msg/Int32"

# phase-267 (non-flat types) — a NESTED message …
[[package.metadata.nros.node.publishes]]
topic = "/header"
type = "std_msgs/msg/Header"
```

phase-267 W1c added that second entry for exactly this test: a topic the bridge
must relay whose type the node never constructs, "so the planner resolves the
`[[bridge]]`'s topic NAME to its ROS type pre-build (no sidecar, no build)".

The chain that now drops it:

1. `planner.rs` (~line 93) appends the manifest-derived SYNTHETIC metadata
   AFTER the sidecar JSON artifacts, deliberately: "so the file artifacts win
   the `(package, component)` dedup … a package shipping both an authoritative
   metadata JSON and a stub component table keeps the file's richer data".
2. `schema_components` dedups by `package::component` and keeps the FIRST — the
   sidecar.
3. The sidecar is now produced by the METADATA PROBE, which records what the
   code actually creates: `nodes[0].publishers = [/chatter]` and nothing else.
   A declaration-only topic is precisely the data a probe CANNOT have.
4. `forwarded_topics` reads `nodes[].entities`, so it yields `["/chatter"]`.
5. The plan's bridge gets `topics: ["/chatter"]` (verified in
   `generated/demo_bringup/nros-bridge-plan/nros-plan.json`), and the generated
   relay declares exactly one subscriber — confirmed against a
   `RUST_LOG=zenoh=debug` router: `0/chatter/std_msgs::msg::dds_::Int32_/*`.
6. `/header` is published to zenoh by the test's talker (the router registers
   `0/header/std_msgs::msg::dds_::Header_`), crosses no bridge, and the ros2
   cyclone subscriber sees nothing.

So the phase-267 declarations were silently demoted from "authoritative
pre-build" to "ignored" the moment the probe started emitting a sidecar for
these components. That is why #0183 could record this lane green on 2026-07-15
and why it is red now with no change to the workspace or the test.

### FIXED (2026-08-04)

`merge_declared_endpoints_into_winners` in `planner.rs` folds manifest-declared
topic endpoints into the artifact that wins the first-per-id dedup, matching by
TOPIC NAME: an endpoint the winner already describes is left alone (the
sidecar's qos / callback / slot data is the better record), one the winner never
mentions is appended to its first node in the probe's own item shape, so
`collect_schema_endpoint_array` reads it with no special-casing downstream.

The "file artifacts win" rule stays intact for overlapping data; it just no
longer discards data only the manifest can carry.

Verified end to end:

- `nros sync examples/workspaces/bridge-cyclonedds` -> the plan's bridge is now
  `topics: ["/chatter", "/header"]` (was `["/chatter"]`).
- `declarative_zenoh_to_cyclonedds_nested_header_to_ros2` PASSES, and the
  sibling `..._bridge_to_nano_listener` still passes (no regression on the
  overlapping-topic path).
- unit test `declared_only_publish_survives_a_probe_sidecar` pins both halves:
  the declared-only topic is folded in, an overlapping topic is not duplicated,
  and the folded endpoint keeps its declared type so the bridge can resolve it.
- the planner's 42 unit tests pass.

### Fix direction (as originally recorded)

The precedence rule is right for overlapping data and wrong for disjoint data.
A declared topic endpoint the probe cannot observe should be UNIONed into the
winning artifact rather than dropped — merge same-id artifacts' topic endpoints
by topic name, keeping the sidecar's richer fields where both describe the same
endpoint.

Not landed here deliberately: this is the planner's metadata precedence, and
phase-330 / phase-335 are actively rewriting that area in parallel sessions. A
precedence change wants to land with whoever owns that work, plus a regression
test (`a declared-only publish survives a probe sidecar`). The sibling lane
`declarative_zenoh_to_cyclonedds_bridge_to_nano_listener` passes, so the bridge
runtime itself is healthy — this is purely what the planner tells it to relay.

## Evidence

Three runs, narrowing the cause each time:

1. **Full tier-1 `test-all`** in the ROS distrobox — 1244 tests: 1208 passed,
   25 skipped, 11 failed.
2. **Re-run of the 11 serially** (`--test-threads=1`) after a rebase: all 11
   "failed", but 10 were `[SKIPPED] … fixture is STALE` against
   `packages/core/nros-macros/src/main_macro.rs`, which the rebase had changed.
   Nothing could be concluded from this run — recorded because it is the trap:
   a stale fixture reports as a failure and looks like a product bug.
3. **Rebuild every native fixture from the current tree, then re-run the 11
   serially** — 2 passed, 9 failed. The 2 that passed
   (`realtime_tiers::case_01_native_rust`, `large_msg::test_xrce_e2e_integrity`)
   were load flakes in run 1. Of the 9, one is a lane-coverage gap (#0407);
   the other 7 are the table above, plus #0382, with real diagnostics rather than fixture
   complaints.

Serial execution matters: run 1 was a full parallel sweep, which is where this
repo's known QEMU/port flakiness lives. These 8 survive that control.

## NOT caused by the issue-0383 `-Werror` work

The commits pushed alongside this investigation are build-system guards —
C compile flags, CMake cache invalidation, box env/store paths. None can change
runtime message delivery, and every affected fixture compiled successfully
(a `-Werror` regression is a build failure, not a silent delivery failure).

## What has NOT been done

No bisect. Confirming phase-330 W7 as the cause means rebuilding the native
fixture set on the pre-W7 commit and re-running these 8 (~1 h). That is the
next step if the #0398 link is not already enough for whoever holds phase-330.

## Notes

Found finishing tier 1 for the issue-0383 work (2026-08-03/04). `just check` is
fully green in the box on the same tree; this is the test half only.
