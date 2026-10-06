# Phase 479 -- logger hierarchy: `get_child`, level inheritance, release-aware `/rosout`

**Status (2026-10-06). COMPLETE -- W1-W6 landed and the phase is archived.**
W1-W4 as #1706, W5 as #1704, W6 as #1722 (smoke fixtures) plus the closing docs
change (the C++ porting-guide row, the last `dynamic-loggers-<N>` mentions,
RFC-0102 marked Stable). Follow-on outside this phase: issue 1589 put the
`/rosout` bridge, and with it D4's release scope, on C and C++; issue 1037 moves
the remaining `nros-log` Cargo-feature families onto the ladder W5 used. Implements
[RFC-0102](../../design/0102-logger-hierarchy.md). The design, the upstream
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

### W2 -- `create_child` / `get_child` on every surface

- Rust `Logger::create_child(&'static self, child_name: impl Borrow<str>) ->
  Result<&'static Logger, ChildError>` (RFC-0102 D1), `ChildError` =
  `NameTooLong { len, max }` | `ArenaFull`.
- C `nros_logger_get_child`; header regenerated (`just regen-c-headers`).
- C++ `rclcpp::Logger::get_child(const std::string&)` + `(const char*)`, separate
  emit and level handles (RFC-0102 D3).
- On every surface, a child of the catch-all is a top-level name (`x`, not
  `nros.x`), as rclrs does for its empty-named default logger.

**Acceptance:** Rust unit tests for both `ChildError` arms and the catch-all
rule; a C++ runtime probe in `just check cpp` (child name, inherited level, own
level, idempotence); a C test beside `named_logger_levels.c`.

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

- `NROS_LOG_DYNAMIC_LOGGERS` knob, read by a new `nros-log/build.rs` on the
  RFC-0049 ladder (image env > Kconfig / `[board.knobs.*]` > 16), delivered on
  the cargo, cmake and west roads; `CONFIG_NROS_LOG_DYNAMIC_LOGGERS` paired on
  Zephyr.
- Linux host boards state 32, MCU boards keep 16.
- The `dynamic-loggers-<N>` features deprecated (one release), then removed.
- `dynamic_loggers_in_use()` / capacity in the boot report and `just mem-report`.

**Acceptance:** the knob gates (`check-kconfig-knob-forwarding`,
`check-knob-single-reader`) cover the new knob; an image `env` override beats the
board's value in a built image; the count visible in a native boot report and in
`mem-report` output.

### W6 -- docs and ledger

- Ledger rows `rust:Logger::create_child`, `cpp:Logger::get_child` -> adopt-bounded
  with envelopes stating D3/D4's bounds; `c:log_severity_t` loses its "permanent"
  ancestry bound; `cpp:Logger` drops the stale `get_child` reason.
- The `nros-log` `DEFAULT_LEVEL` doc comment, the porting guide and
  `book/src/user-guide/logging.md`.
- Per-RTOS logging smoke fixtures (`logging-smoke-*`) print a child record, and
  `logging_smoke.rs` asserts the dotted name reaches each platform's output.
  **Done (2026-10-06)**: all seven print `smoke.child: child payload`; measured passing on freertos-mps2, mps2-baremetal, nuttx-qemu-arm, threadx-linux, threadx-riscv64 and zephyr native_sim (esp32 image builds; no esp32c3 QEMU on the measuring host).
