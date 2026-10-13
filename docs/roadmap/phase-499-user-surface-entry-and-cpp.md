# Phase 499 — The user surface is upstream's: include paths, `main`, and the C++ examples

**Status (2026-10-14). W1–W5, W7, W8 landed (W6 folded into W4); RTOS images other than ThreadX-Linux and bare metal are left to the tier-2 lane.** Implements the
[RFC-0089](../design/0089-ros2-api-adoption-and-the-compile-or-conform-rule.md)
amendment "the user surface is upstream's — include paths, `main`, and no
`nros` spelling" (2026-10-13). Follows
[phase-492](phase-492-c-api-rcl-names.md), which renamed the C API and moved C
user code onto upstream include paths (`check-c-user-includes`, which W5 grew
into `check-user-surface`).

## Why

An rclc, micro-ROS or rclcpp program should port to nano-ros with no nano-ros
vocabulary. After phase-492 a C user still writes `nros_app_main` +
`NROS_APP_MAIN_REGISTER()`, threads `NROS_ENTRY_LOCATOR` /
`NROS_ENTRY_DOMAIN_ID` and a `getenv("NROS_LOCATOR")` into
`rclc_support_init_with_locator`, and on bare metal hand-writes
`nros_app_register_backends()`. A C++ user includes `<nros/nros.hpp>`,
`<nros/component.hpp>` and `<nros/app_main.h>`, and writes `NROS_TRY_RET`.

Measured in `examples/` (2026-10-13, `origin/main`):

| Name | Uses | Upstream counterpart | Verdict (maintainer, 2026-10-13) |
|---|---|---|---|
| `nros_app_main` | 171 | `int main(int, char**)` (rclc, rclcpp); micro-ROS templates `appMain` | adopt `main` (W5) |
| `NROS_APP_MAIN_REGISTER()` | 161 | none | retire (W5) |
| `nros_app_register_backends()` (hand-written: bare metal 12, PX4 1) | 41 | none (RMW by link / `RMW_IMPLEMENTATION`) | generate on every road (W3) |
| `NROS_ENTRY_LOCATOR` / `NROS_ENTRY_DOMAIN_ID` + `getenv("NROS_LOCATOR")` | 138 / 46 | `rclc_support_init(&s, argc, argv, &alloc)`; `ROS_DOMAIN_ID` | out of user code (W2) |
| `<nros/nros.hpp>` | 86 | `<rclcpp/rclcpp.hpp>` (shipped, same umbrella) | adopt (W1) |
| `<nros/app_main.h>` in C++ | 41 | `main` + `rclcpp::init(argc, argv)` | adopt with W5 |
| `<nros/component.hpp>` | 33 | `RCLCPP_COMPONENTS_REGISTER_NODE` (different contract) | fold into the umbrella (W1) |
| `<nros/log.h>` + `RCLC_LOG_*` in C++ | 6 | `RCLCPP_INFO(...)` | adopt (W1) |
| `<nros/log.hpp>`, `subscription.hpp`, `parameter.hpp` | 4 | `<rclcpp/*.hpp>` | umbrella (W1) |
| `<nros/main.hpp>` | 3 (proof-of-concept examples) | none, entry-internal | internal (W7) |
| `<nros/nros_cpp_ffi.h>` + `nros_cpp_integrity_status_t` | 2 + 8 | none (E2E safety) | `rclcpp::` wrapper (W6) |
| `NROS_TRY_RET` / `NROS_TRY_LOG` | 113 / 34 | none (rclcpp throws) | `RCLCPP_TRY_RET` / `RCLCPP_TRY_LOG` (W8) |
| `nros_cpp_init_multi` (PX4 bridge) | 4 | none | keep (seam) |
| `NROS_CPP_STD`, `NROS_BRIDGE_RMW`, `NROS_RUNTIME_OVERLAY`, `NROS_TEST_*` | — | none | keep (build/test facts) |

## Work items

**W1 — C++ examples include `<rclcpp/rclcpp.hpp>` only.**
The umbrella gains the typed-component support `<nros/component.hpp>` carries
(as the C umbrella gained `component.h`). C++ examples, the book's C++
snippets, the `nros new` C++ scaffolds and the generated C++ entry's
user-visible includes drop `<nros/*.hpp>`, and C++ files that log through
`RCLC_LOG_*` move to `RCLCPP_*`. Extend `check-c-user-includes` (W5: `check-user-surface`) to C++ sources and ```cpp blocks, with the internal set derived
from `packages/api/nros-cpp/include/nros/` and `packages/api/nros-c/include/nros/`.
*Acceptance:* no user-facing C++ file includes an internal header; the gate
fails on main's tree.

**W2 — C examples call `rclc_support_init(&support, argc, argv, &allocator)`.**
It already resolves locator and domain from the build and the environment.
Drop the `getenv("NROS_LOCATOR")` / `NROS_ENTRY_*` boilerplate from every
example; keep `rclc_support_init_with_locator` only where an example exists to
show an explicit locator. *Acceptance:* `NROS_ENTRY_LOCATOR` /
`NROS_ENTRY_DOMAIN_ID` appear in no example; native C e2e tests pass.

**W3 — `nros_app_register_backends()` is generated on every road.**
Bare metal (mps2-an385) and the PX4 module emit it from the image's RMW as
`nano_ros_link_rmw` does for cmake. *Acceptance:* no example defines it; the
bare-metal and PX4 builds link.

**W4 — the C++ `rclcpp::` wrapper for E2E integrity status.**
An `rclcpp::` type over `nros_cpp_integrity_status_t`; the safety examples
stop including `<nros/nros_cpp_ffi.h>`.

**W5 — the application writes `main`.**
Where the board owns `main` (FreeRTOS, ThreadX, bare metal; NuttX already
renames), the build compiles the app's TUs with `-Dmain=nros_app_main`; the
umbrellas declare `extern "C" int nros_app_main(int, char**)` so a C++ `main`
takes C linkage. `NROS_APP_MAIN_REGISTER()` and the shim header retire; on
POSIX and Zephyr the user's `main` is the entry. Measure which `main`
signatures each language and road accepts, and make an unsupported one a
build-time refusal naming the form, never a link error. C++ examples move to
`rclcpp::init(argc, argv)` where they do not need `init_in`.
The materialized-entry sentinel the book showed
(`#include <nros/main.hpp>` + `NROS_MAIN(Board, "bringup:launch")`) was
user-visible too.
*Acceptance:* no example writes `nros_app_main` or `NROS_APP_MAIN_REGISTER`;
every road's example builds link; native e2e tests pass; Zephyr/NuttX/ThreadX/
FreeRTOS images built by the tier-2 lane.

**W6 — Safety examples on the W4 wrapper.** (Folded into W4 if small.)

**W7 — The `<nros/main.hpp>` proof-of-concept examples.**
`component-poc`, `component-node-poc`, `transform-poc` reach entry internals;
move them under `packages/testing/` or rewrite them onto the public surface.

**W8 — `NROS_TRY_RET` / `NROS_TRY_LOG` → `RCLCPP_TRY_RET` / `RCLCPP_TRY_LOG`.**
No forwarders (as phase-492). The user-defined `RCLCPP_TRY_LOG` hook keeps its
role. *Acceptance:* no `NROS_TRY_` spelling outside archived docs.

## Out of scope

- Renaming the internal `nros/*.h` / `nros/*.hpp` headers themselves into
  `rclc` / `rclcpp` (the maintainer's later step).
- Build knobs, test knobs, port seams, CMake function names.

## Progress

### 2026-10-14 — W1–W8

- **W8** `NROS_TRY` / `NROS_TRY_RET` / `NROS_TRY_LOG` → `RCLCPP_TRY*`. Also fixed
  a phase-492 defect: the C++ void-returning form had become `RCLC_CHECK`,
  which `nros-c`'s `check.h` defines with a different argument type. It is
  `RCLCPP_CHECK`.
- **W1** 87 C++ example sources, the book's C++ snippets and the `nros new` C++
  component include only `<rclcpp/rclcpp.hpp>` (already the same umbrella).
- **W2** 40 C examples call `rclc_support_init(&support, argc, argv, &allocator)`;
  bare-metal C++ calls `rclcpp::init_in()`. **Found by the bare-metal QEMU
  e2e, not by reading**: `NROS_ENTRY_LOCATOR` / `NROS_ENTRY_DOMAIN_ID` are
  macros in the caller's TU, which C++'s inline `init_in()` reads and the Rust
  `rclc_support_init` cannot, so every bare-metal C image hit
  `ConnectionFailed`. `<nros/baked_rmw.h>` now always forwards the baked
  locator and domain (an empty locator stays "absent"; the environment still
  wins). The `check c` probe asserts both.
- **W3** `nros-c` publishes its enabled RMW backends over its `links` key
  (derived from the `rmw-*` features, never a name list: RFC-0071), and
  `nros_cc_flags::backends` generates `nros_app_register_backends()` for a
  cargo-rooted image; the 12 bare-metal hooks went. PX4 already generated it.
- **W4** (with W6) `rclcpp::IntegrityStatus` (a typedef of the FFI struct)
  and `rclcpp::Node::start_parameter_services()`; the safety and parameter
  examples touch no `nros_cpp_*`.
- **W5** every plain `nano_ros_add_executable` compiles its sources with
  `main=nros_ported_main` and generates the `nros_app_main` forwarder +
  `NROS_APP_MAIN_REGISTER()` (phase-482 W3's `ROS2_MAIN`, generalised);
  `nros_cc_flags::app_main` does it for a cargo-rooted image. 84 examples and
  5 test bins write `main`. Measured deviation: a C++ `main` must be exactly
  `int main(int argc, char** argv)`; another form fails at LINK naming
  `nros_ported_main(int, char**)`, not at compile time. The book's
  `NROS_MAIN` sentinel (an optional marker nothing consumes) was dropped from
  the user surface rather than given a new name.
- **W7** the three entry-machinery POCs moved to `packages/testing/nros-tests/bins/`.
- Gate `check-user-surface` (was `check-c-user-includes`): C and C++.
- Verified locally: native C/C++ (pub/sub, service, action, C↔C++ interop,
  parameters), bare-metal QEMU C/C++ (6/6), ThreadX-Linux exit status and
  SIGTERM, the C/C++/mixed/features/safety workspaces (features 13/13 C/C++/
  mixed cases, parameter live read 3/3). Not built here: FreeRTOS, NuttX,
  Zephyr, ThreadX-RISC-V images (tier-2 lane).
