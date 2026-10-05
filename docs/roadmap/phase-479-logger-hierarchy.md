# Phase 479 -- logger hierarchy: `get_child`, level inheritance, release-aware `/rosout`

**Status (2026-10-05). PROPOSED -- nothing implemented.** Implements
[RFC-0102](../design/0102-logger-hierarchy.md). The design, the upstream
measurements and the three bounds (D3 overflow, D4 `/rosout`, D5 arena size) live
there; this doc is the work breakdown and the acceptance list.

**Prior:** issue 1682 (`rclcpp::Logger` owns its name -- the prerequisite for a C++
child), phase-417 W4.d (`get_or_create_logger` and the dynamic-logger arena),
phase-467 (the ledger rows this reverses: `c:log_severity_t`,
`rust:Logger::create_child`, `cpp:Logger::get_child`), issue 1019 (per-logger
dispatch, and the shared-threshold aliasing D3 avoids).

## Work items

### W1 -- parent pointer and effective level (`nros-log`)

- `Logger` gains `parent: AtomicPtr<Logger>`; `const fn new` / `with_level`
  initialise it null.
- `is_enabled` / `level()` resolve own -> ancestors -> `default_level()`.
- `get_or_create_logger` links a dotted name to its nearest existing ancestor, and
  re-points existing loggers a new logger now sits closer to (RFC-0102 D2).

**Acceptance:** unit tests -- parent `Debug` makes an unset child emit `Debug`; a
child's own level overrides the parent; an ancestor created AFTER its descendant
is linked to it; a middle ancestor created later re-points the grandchild; the
chain walk stops at a set level and never loops.

### W2 -- `child` / `get_child` on every surface

- Rust `Logger::child` (spelling per RFC-0102 open question 1).
- C `nros_logger_get_child`; header regenerated (`just regen-c-headers`).
- C++ `rclcpp::Logger::get_child(const std::string&)` + `(const char*)`, separate
  emit and level handles (RFC-0102 D3).

**Acceptance:** a C++ runtime probe in `just check cpp` (child name, inherited
level, own level, idempotence); a C test beside `named_logger_levels.c`.

### W3 -- overflow takes the parent, and `set_level` refuses (RFC-0102 D3)

**Acceptance:** a 49-byte child name and an exhausted arena each emit under the
parent's name at the parent's level, `set_level` on the child returns an error and
leaves the parent's and the catch-all's levels unchanged, and the warning appears
once.

### W4 -- `/rosout` follows the release (RFC-0102 D4)

- Node-logger marker set by `Node::logger()`.
- nros-node hands the release policy (`ros-humble` / `ros-iron` / `ros-jazzy`) to
  `nros_log::rosout`.
- The sink publishes per D4; free loggers stop being published.

**Acceptance:** sink unit tests for each release x {node logger, child, free
logger}; one interop cell under `ros-jazzy` that sees a child record on `/rosout`
from a real `ros2 topic echo`.

### W5 -- arena size per board, with one owner, reported (RFC-0102 D5)

- Linux host boards 32, MCU boards 16, overridable per image.
- Conflicting `dynamic-loggers-<N>` selections in one graph fail the build instead
  of silently picking the smallest.
- `dynamic_loggers_in_use()` / capacity in the boot report and `just mem-report`.

**Acceptance:** a gate for the single-owner rule; the count visible in a native
boot report and in `mem-report` output.

### W6 -- docs and ledger

- Ledger rows `rust:Logger::create_child`, `cpp:Logger::get_child` -> adopt-bounded
  with envelopes stating D3/D4's bounds; `c:log_severity_t` loses its "permanent"
  ancestry bound; `cpp:Logger` drops the stale `get_child` reason.
- The `nros-log` `DEFAULT_LEVEL` doc comment, the porting guide and
  `book/src/user-guide/logging.md`.
- Per-RTOS logging smoke fixtures (`logging-smoke-*`) print a child record, and
  `logging_smoke.rs` asserts the dotted name reaches each platform's output.
