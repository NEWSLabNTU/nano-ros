# Phase 504 — the C++ API's headers move into `rclcpp/`

**Status (2026-10-17). Complete — W1–W5 landed in #2278; the index pins
release 0.6.0-nros3.** Landed as phase-503, which
[phase-503](../phase-503-route-deadline-monitor.md) (the route deadline monitor)
had already taken; renumbered to 504 at archive time, so commits on main still
say phase-503. The C++ half of the RFC-0089 amendment "the C headers
live under `rclc/`" (2026-10-15), which
[phase-501](phase-501-c-api-headers-move-into-rclc.md) left as a follow-up
(its ruling 3).

## Why

Since phase-483 the C++ user API is declared in `rclcpp::`,
`rclcpp_action::` and `rclcpp_lifecycle::`, and since phase-499 a user
includes only upstream's paths. The declarations still live in
`packages/api/nros-cpp/include/nros/`, and `<rclcpp/rclcpp.hpp>` is an
11-line file that includes `nros/nros.hpp`. So the C++ API has two homes, as
the C API had before phase-501: the path a user includes and the path that
holds the content.

Measured on this branch (phase-501's head, 2026-10-16):

- `include/nros/` holds 59 C++ headers and 3 C-ABI headers
  (`nros_cpp_ffi.h` and the per-build sizes header with its buildless
  snapshot).
- About 200 include sites name one of them: 116 in nros-cpp itself, 35 in the
  entry generator (`nros-cli-core`), 11 in `rosidl-codegen`'s C++ templates,
  16 in `nros-tests`, the rest in gates, cmake and docs. No file under
  `examples/` includes one.
- No `nros::` namespace is left (`check-cpp-no-nros-namespace`), so this
  phase moves files and rewrites includes. It renames no identifier.

## Rulings

Phase-501's rulings 1 and 2 (maintainer, 2026-10-15) apply unchanged, with
`rclcpp` for `rclc`:

1. **Layout: `rclcpp/` owns the content.** A header with an upstream
   counterpart takes upstream's path, in the package whose namespace it
   declares (`rclcpp_action/`, `rclcpp_lifecycle/`). A user-reachable header
   with no counterpart becomes `rclcpp/<x>.hpp`. Generated, configuration,
   entry and internal support headers go to `rclcpp/detail/`. No `nros/`
   directory remains in nros-cpp, and no forwarding headers stay behind.
2. **Identifiers: only what a user writes.** None are renamed here.

## Header map

| From `nros/` | To | Why |
|---|---|---|
| `nros.hpp` (umbrella) | `rclcpp/rclcpp.hpp` | upstream's umbrella; replaces the 11-line forwarder |
| `node` `publisher` `subscription` `client` `service` `timer` `executor` `qos` `clock` `duration` `time` `parameter` `guard_condition` `callback_group` `event` | `rclcpp/<same>.hpp` | upstream has the header |
| `log.hpp` | `rclcpp/logging.hpp` | upstream's name |
| `action_client.hpp`, `action_server.hpp` | `rclcpp_action/client.hpp`, `rclcpp_action/server.hpp` | they declare `rclcpp_action::` |
| `lifecycle.hpp` | `rclcpp_lifecycle/lifecycle_node.hpp` | replaces that forwarder; the other three `rclcpp_lifecycle/` forwarders include it |
| `options` `future` `graph` `node_parameters` `polling_{subscription,service,client,action_server,action_client}` `stream` `tick_ctx` `sched_context` `rosout` `bridge` `transport` `result` `span` `owned` `handle` `{client,service,subscription}_handle` `inplace_fn` `fixed_{string,sequence}` `heap_{string,sequence}` `size_bound` | `rclcpp/<same>.hpp` | no upstream counterpart; user-reachable through the umbrella |
| `main` `component` `config` `std_detect` `traits` `hosted_block` `callback_context` `entity_name` `serialization_format` `declared_qos` `declared_params` | `rclcpp/detail/<same>.hpp` | entry glue, generated-code support and internals |
| `nros_cpp_ffi.h` (cbindgen, committed) | `rclcpp/detail/nros_cpp_ffi.h` | generated |
| `nros_cpp_config_generated.h`, `nros_cpp_config_generated_buildless.h` | `rclcpp/detail/<same>.h` | generated; `CPP_HEADER_SUBDIR` in `nros_build_paths::artifact` becomes `rclcpp/detail` |

The two CLI-written headers phase-501 left under `nros/` move too:
`nros_declared_params_generated.h` (C++ only) to `rclcpp/detail/`, and
`nros_declared_qos_generated.h`, which both languages include, to
`rclc/detail/`, beside the other C-includable generated headers.

## Work items

- **W1 — hand-written content moves.** `git mv` per the map, rewrite every
  include. Include guards take the new path. Acceptance: `just check cpp`;
  `git grep` finds no include of a moved `nros/` name.
- **W2 — generated headers move.** cbindgen's output path, the sizes header
  (`CPP_HEADER_SUBDIR`, the cmake mirror, the `links` channel, the buildless
  snapshot), and the declared-QoS / declared-params headers. Acceptance:
  `check-sizes-header-mirrors`; one Zephyr and one NuttX C++ image build.
- **W3 — the emitters follow.** `rosidl-codegen`'s C++ templates and the entry
  generator emit the new paths. `NROS_CODEGEN_VERSION` and its minimum move to
  18, so a version-17 C++ tree regenerates instead of failing to include
  `<nros/heap_string.hpp>`. That bump puts the index's pinned release outside
  the accepted range again, so this phase needs a release (`0.6.0-nros3` or
  later) before it can merge, as phase-501 did.
- **W4 — gates.** `check-user-surface` derives nros-cpp's internal set from
  `rclcpp/detail/`, and refuses `<rclcpp/detail/*>` in user code. Every gate
  that scoped nros-cpp to `include/nros/` follows. Each keeps its negative
  control.
- **W5 — docs.** `book/src/`, `docs/guides/cpp-api.md`,
  `docs/reference/c-api-cmake.md`, AGENTS.md "C/C++ Integration", RFC-0018
  and RFC-0089. Archived docs keep their historical paths.

## Out of scope

- Port-seam headers other packages own (`<nros/platform.h>`,
  `<nros/rmw_vtable.h>`, `<nros/transport.h>`).
- An `rclcpp_action/rclcpp_action.hpp` umbrella. Upstream has one; nothing
  here includes it today.

## Progress

### 2026-10-16 — W1–W5

- **W1** moved the 59 C++ headers and `nros_cpp_ffi.h` per the map, with one
  regex over every tracked file outside `docs/`, `book/`, the changelogs,
  `tmp/` and `third-party/`. The emitters' templates are inside that set,
  so W3's include change landed here too, with the codegen bump to 18 / 18.
- **W2** changed `CPP_HEADER_SUBDIR` to `rclcpp/detail` and moved the sizes
  stub, the buildless snapshot and the two CLI-written declared headers.
- **W4, gates.** The text sweep cannot see a path a gate assembles, and
  most of the misses were exactly that: path joins
  (`check-qos-profile-ssot`, the mirror gate's self-test), shell globs
  (`check-cpp-hosted-minimal-libcpp`, three loops in `check-cpp`, two
  clang-format globs), one-level `listdir`s (api-parity's refusal macros,
  `check-cpp-subscription-bound-supplied`) and a prefix test (api-parity's
  "every TU reads one of our headers"). Each would have scanned an empty
  directory and, where it had no vacuity floor, passed. The sweep also
  rewrote three self-test samples into nonsense, fixed by hand. One gate
  had been narrower than its rule since phase-501:
  `check-config-fallback-macros` globbed `include/nros/`, so it checked
  nros-cpp's buildless snapshot and never nros-c's. It reads both now.
- **W5, docs.** `book/src/`, `docs/guides/`, `docs/reference/` (the
  api-parity ledger shards included), RFC-0089 (its C++ follow-up note),
  RFC-0018 and RFC-0096. Audit findings, issues, research notes and other
  phases are history and keep their paths.
