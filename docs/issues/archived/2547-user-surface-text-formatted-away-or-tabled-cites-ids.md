---
id: 2547
title: "User-surface messages formatted into a variable, or kept in message tables, can still cite internal ids"
status: resolved
type: tech-debt
severity: low
area: [cli, ux, logging, runtime]
related: [issue-2443, issue-2551, phase-523, phase-552]
found: 2026-11-14
resolved: 2026-11-14
resolved_in: "#3815 (the subject and its sweep, one change)"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md) W5.**
The gaps [issue 2443](2443-runtime-log-and-colcon-messages-cite-ids.md)
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

## Progress

**2026-11-14 — the subject, and the sweep that empties it.**
`check-user-surface-ids` reads a ninth source subject, **text that reaches a
print later** (`deferred_hits()`), over the runtime subject's files plus the
build scripts. It shares the lexer (`code_and_literals`, `enclosing_calls`)
and leaves any literal another subject already reads to that subject, so no
line is reported twice. Four readings, each self-tested both ways:

- **(a) Dataflow-lite, Rust, per function.** A literal in the value of
  `let x = …`, or written into `x` later (`push_str`, `push`, `insert_str`,
  `write!`/`writeln!`, `+=`, `x = …`), is read when `x` later, in the same
  function body, is an argument of a call a subject reads (print, log, panic,
  `Err(…)`, `bail!`, `.context`, `syn::Error::new`, `compile_error!`, a build
  script's `warn(…)`/`.note(…)`), sits in a `return` or the tail expression,
  or feeds another binding that does. An inline `{x}` capture counts.
- **(b) Message tables, by convention.** A `const`/`static` named `*_MSG`,
  `*_HINT`, `*_TEXT`, `*_REFUSAL`, `*_TABLE` or `*_TEMPLATE` (plural too); the
  item after a `// user-surface: table` comment; a C `#define NROS_*REFUSE*`,
  `*_MSG`, `*_TEXT`, `*_HINT` or `*_REFUSAL`. The marker was added where the
  name does not carry the convention: `retired_tables.rs`
  (`RETIRED_IMAGE_KEYS`, `RETIRED_GROUP_TIERS`), `nros-entry-lower`'s board
  hints (`RETIRED_BOARD_KEYS`, `REMOVED_BOARD_KEYS`, `ESP32_REMOVED`), the
  orchestration `CPP_ONLY_FRAMEWORKS`, `RETIRED_FRAMEWORKS` and
  `MOVED_NETWORK_KEYS`, `nros::init`'s `REFUSE_INIT_ARGS`, `nros::main!`'s
  `RETIRED_BOARD_ARG` and `rclcpp/logging.hpp`'s
  `NROS_RCLCPP_ABORT_FAILED_CREATE`.
- **(c) Generated-file headers.** The leading comment lines of a literal
  (as the emitted file holds them) that say `generated`, `do not edit` or
  `written by`, and the leading comment lines of the literals that continue
  that header. The body below a header is not read.
- **(d) C/C++ message macros.** A `#define NAME(params)` whose body hands a
  parameter to a print or log call or a `static_assert` is a printer; a
  literal passed in that place is read (`NROS_RCL_COMPAT_ASSERT`,
  `NROS_STATIC_ASSERT`).

Measured on the tree before the sweep, with the final readings: **126 ids in
40 files** (the issue's own heuristic count, 131 in 47, split them
differently). By file: `rclcpp/logging.hpp` 39, `retired_tables.rs` 10,
`rtos_realizer.rs` 6, `rcl_compat.h` 5, `levels.rs` 5, `knob_registry.rs` 4,
`nros-build-helpers` `cpp.rs` 4, `nros-sizing-descriptor` `render.rs` 4, the
ThreadX-Linux board build script, `bounds.rs`, `main_macro.rs` and
`session_properties.rs` 3 each, and 1 or 2 in 28 more. The sweep moved each
id to a comment above its text and kept the text saying what and how to fix
(the retired-table `by` column now names the release or what replaced the
table). It also fixed the id-bearing user text in the touched files that the
readings still miss (the launcher's install, checkout and CI-pin
refusals, the NuttX GCC-floor refusal, the parameter-slot and bridge
refusals, the platform-config refusals, a `cfg_attr` deprecation note and
the parameter-overflow log): 180 ids in 52 files in all. The knob registry's
retired-spelling texts are generated, so they changed in
`scripts/gen-knob-registry.py` and `knobs.toml` and both outputs were
regenerated. Measured after: **0**.

The compile-time subject read only the first line of a quoted `#error` whose
message continues with `\`-newline (the continuation is blanked in the code
it matched against); it now matches the directive on the text, and the one id
that hid there (`nros-board-threadx-qemu-riscv64/startup.c`) is fixed.

Started as the per-file ratchet the gate plan proposed, the subject reached
zero in the same change, so it lands as a baseline-zero rule with no counted
baseline file (one that reaches zero and stays a ratchet can regress
unwatched, which is why issue 2443's ratchets were dropped once empty).

Gaps, stated in the checker: flow across functions (a value returned and
printed by the caller is read only at its `return`), a literal returned bare
(`fn hint() -> &str { "…" }`), a buffer handed in as a parameter, a struct
field (`Refusal { hint: x }`), a `Vec` drained by a `for` loop into a print,
a table with neither a convention name nor the marker, C text built into a
buffer by anything but `snprintf`, C++ stream output, and a generated file's
body below its header (the doc comments a build script writes above the
constants it emits into `OUT_DIR` still carry ids; they are rustdoc of
generated constants, not output).

## Resolution

Every shape this issue measured is read by `check-user-surface-ids` at
baseline zero, with no allowlist: text formatted into a binding or buffer and
printed, returned or raised later, message tables by name or marker, C/C++
message macros and wrapper macros, and generated-file headers. The sweep in
the same change took the 126 ids those readings found (and 54 more the
readings still miss, in the same files) to zero. What the readings cannot
see is stated as the checker's gaps above, and is tracked as
[issue 2551](../2551-user-surface-text-beyond-function-flow-cites-ids.md), with a gate plan.
