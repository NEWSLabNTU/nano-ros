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

## Bridge — root cause identified, NOT fixed

`declarative_zenoh_to_cyclonedds_nested_header_to_ros2` expects a `Header` on
`/header` to cross the bridge. It cannot, as configured:

- `bridge-cyclonedds`'s `system.toml` declares ONE component,
  `talker_pkg::Talker`, which publishes `std_msgs/Int32` on `/chatter` only —
  no `/header` anywhere in the workspace, before OR after the phase-331 W6
  rename (`ws-bridge-rust` -> `bridge-cyclonedds`, verified against the
  pre-rename tree).
- the generated model's `execution.bridges` carries no topic list, and
  `bridge_gen.rs` states the planner "only forwards declared topics"
  (`expect("planner only forwards declared topics")`).
- observed: with the router at `RUST_LOG=zenoh=debug`, the bridge declares
  exactly ONE subscriber, `0/chatter/std_msgs::msg::dds_::Int32_/*`. No
  `/header` subscriber, and no `/header` resource is registered at all — even
  though the `header-chatter-talker` fixture logs `Published Header: N`.

So either the relay is meant to forward undeclared topics (and regressed to
declared-only), or the test's premise needs the workspace to declare `/header`.
Archived issue #0183 records this exact lane PASSING on 2026-07-15, so it did
work once — that is the thread to pull.

The last point deserves attention on its own: the talker reports publishing
Headers that never reach the router as a resource.

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
