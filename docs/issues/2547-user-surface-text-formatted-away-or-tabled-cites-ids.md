---
id: 2547
title: "User-surface messages formatted into a variable, or kept in message tables, can still cite internal ids"
status: open
type: tech-debt
severity: low
area: [cli, ux, logging, runtime]
related: [issue-2443, phase-523]
found: 2026-11-14
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md) W5.**
The gaps [issue 2443](archived/2443-runtime-log-and-colcon-messages-cite-ids.md)
closed with, stated in `scripts/check/check-user-surface-ids.py` and filed
here so the archive does not lose them.

## What

RFC-0116 D2 says the user surface names an action, never an internal id.
`check-user-surface-ids` reads every text class issue 2443 named at baseline
zero: the CLI library, build-script panics and warnings, the colcon plugin,
user-run shell scripts, runtime log and panic text of every package and
example, compile-time text, and error text assembled away from its print
(`Display` impls, `#[error]`, `Err(…)` and the error builders). Each subject
decides by the call a literal sits in. A literal that reaches a user through
a step in between is read by none of them.

Measured 2026-11-14, after issue 2443's last fix: **131 id-bearing string
literals in 47 files** of `packages/` and `examples/` (test-only code, vendored
trees and the CLI library left out, as the gate leaves them out) sit in no
call any subject reads. Not all of them reach a user, which is why this is
measured and not yet gated. By shape (the measurement's own heuristics, so
the split is approximate):

| shape | count | top files |
| --- | --- | --- |
| C/C++ text kept in a `#define` message macro or passed through a wrapper macro | 38 | `nros-cpp/include/rclcpp/logging.hpp` 31 (the `NROS_RCLCPP_REFUSE_*` texts a `static_assert` or `say_refused` prints), `nros-c/include/rclc/detail/rcl_compat.h` 5 (`NROS_RCL_COMPAT_ASSERT`) |
| text appended or written into a buffer handed on later (`push_str`, `write!`, `writeln!`) | 22 | `nros-orchestration-ir` `levels.rs` 5, `rtos_realizer.rs` 5, `leaf_system.rs` 2, `system_net.rs` 2; `nros-launcher` `pin.rs` 2, `launch.rs` 1 |
| generated-file headers and text an emitter writes | 21 | `rosidl-codegen` `ament_stamp.rs` and `bounds.rs` 2 each, `session_properties.rs` 2, `nros-build-helpers` `cpp.rs` 2 |
| message tables (a `const`/`static` slice or tuple list of names and texts) | 29 | `nros-orchestration-ir` `retired_tables.rs` 11, `nros-entry-lower` `lib.rs` board hints 5, `nros-orchestration-ir` `lib.rs` and `system_net.rs` 2 each, `nros-zephyr-build` `knob_registry.rs` 2, `nros-sizes-build` 2 |
| a `let` binding formatted first and printed or returned later (`let why = format!(…)`) | 13 | `nros-build-helpers` `cpp.rs` 4, `nros-macros` `main_macro.rs` 2, `nros-board-common` `board_net.rs` and `freertos_build.rs` |
| a literal a function returns for its caller to print | 3 | `nros-launcher` `launch.rs` 2, `ament_stamp.rs` 1 |
| other (a helper's argument, a `Some(…)` cell, a `.replace(…)` piece, a `cfg_attr` deprecation) | 5 | `priority_plan.rs` 2, `platform_config.rs`, `knob_registry.rs`, `executor/types.rs` |

Also unread: C++ stream output (`std::cout << "…"`; no id today), and calls
the runtime subject's lists do not name.

## Gate plan

One subject in `check-user-surface-ids.py`, **"text that reaches a print
later"**, sharing the runtime subject's lexer (`code_and_literals`,
`enclosing_calls`) and its file list (`runtime_files()`: every package and
example, the same skips). Three readings, each with its own self-test:

1. **Dataflow-lite, per function.** A `let x = format!(…)` / `String::from`
   / `"…".to_string()` / `let mut x = String::new()` followed by
   `x.push_str(…)` / `write!(x, …)`: if the binding `x` later appears, in the
   same function body, as an argument of a call any subject reads (a log or
   print, `panic!`, `Err(…)`, `bail!`, `.context`, a board print helper) or
   as the function's tail expression or `return`, every literal that fed it is
   read. The function body is the `{…}` the lexer already brackets; no
   cross-function flow.
2. **Message tables, by convention.** A `const` or `static` whose name ends
   in `_MSG`, `_HINT`, `_TEXT`, `_REFUSAL` or `_TABLE`, a table marked with a
   `// user-surface: table` comment the gate looks for, and in C/C++ a
   `#define NROS_*_REFUSE*` / `*_MSG` / `*_TEXT` macro body. The convention
   is added to `retired_tables.rs`, `nros-entry-lower`'s board hints and
   `rclcpp/logging.hpp` in the same change, so the rule starts with readers.
3. **Generated-file headers.** A literal written by an emitter whose text
   begins with a comment leader (`//`, `/*`, `#`, `--`) and says
   `generated`/`do not edit`: the header lands in the user's tree.

Rollout: start the subject as a **per-file ratchet** (counts in a
`.config/` JSON the gate reads, the shape issue 2443 used for the CLI
library), sweep it file by file starting with `rclcpp/logging.hpp`,
`retired_tables.rs`, `levels.rs`, `rtos_realizer.rs` and `nros-launcher`,
and turn it into a baseline-zero rule once the sweep empties it. Literals the
readings above still miss stay stated gaps in the docstring.

## Fix direction

The same as issue 2443's: each id moves to a `//` (or `/* */`, `#`) comment
above its literal, and the text says what went wrong and what to do.
