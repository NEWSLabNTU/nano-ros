---
rfc: 0102
title: "Logger hierarchy: get_child, level inheritance, and what each backend sees"
status: Draft
since: 2026-10
last-reviewed: 2026-10
implements-tracked-by: [phase-479]
supersedes: []
superseded-by: null
---

# RFC-0102 — Logger hierarchy: `get_child`, level inheritance, and what each backend sees

**Status:** Draft. Decided 2026-10-05, not yet built; [phase-479](../roadmap/phase-479-logger-hierarchy.md)
carries the work.

## Summary

`rclcpp::Logger::get_child` and rclrs's `Logger::create_child` are adopted. A child
is a logger named `parent.suffix` whose level, when it has none of its own, is
its nearest ancestor's — rcutils's dotted-ancestry rule. The hierarchy is held as
a **parent pointer** on each `nros_log::Logger`, built when a logger is created,
so the hot path never parses a name. Three bounds a `no_std` image imposes are
decided here: whether a child's records reach `/rosout` (follows the ROS
release), what happens to a name over the 48-byte limit (emit through the parent,
refuse `set_level`), and how many runtime loggers an image holds (a per-board
default, reported at runtime).

This reverses the "permanent" bound phase-467 recorded on ledger rows
`c:log_severity_t`, `rust:Logger::create_child` and `cpp:Logger::get_child`.

## Motivation / problem

### Upstream's two behaviours, and the one that matters

Read from the ROS 2 Humble headers in the `ros2` box and from the `iron` branch
of `ros2/rcl` and `ros2/rclcpp` (2026-10-05):

1. **Naming.** `get_logger("abc").get_child("def")` is a logger named `abc.def`.
2. **Levels.** `rcutils_logging_get_logger_effective_level`: "the severity level
   of the logger if it is set, otherwise it is the first specified severity level
   of the logger's ancestors, starting with its closest ancestor … If the level
   has not been set for the logger nor any of its ancestors, the default level is
   used."

A port that writes

```cpp
node->get_logger().set_level(rclcpp::Logger::Level::Debug);
RCLCPP_DEBUG(node->get_logger().get_child("planner"), "…");
```

prints the line upstream. A `get_child` that supplied only the dotted NAME would
drop it — the compile-and-differ RFC-0089 forbids. So naming without inheritance
is worse than no `get_child`, and inheritance is the substance of this RFC.

### Why it was recorded as impossible, and why that no longer holds

Phase-467 made the missing ancestry walk permanent on `c:log_severity_t` because
(a) no allocator could build `parent.child` and (b) `nros_log`'s 32-slot intern
table had nowhere to store a runtime-built name. Both have since gone:

- `nros_log::get_or_create_logger` (phase-417 W4.d) copies runtime names into a
  static arena (`pool::intern_name`), sized by the `dynamic-loggers-<N>` feature.
- `parent.child` is at most `MAX_LOGGER_NAME_LEN` (48) bytes, so it concatenates
  into a stack buffer; no allocator is involved.
- `rclcpp::Logger` owns its name since issue 1682 (`FixedString<128 + 1>`), so the
  C++ value can hold the child's full name.

The phase-467 rationale also said an ancestor would have to occupy a slot to
carry a level. True, and harmless: the ancestors a `get_child` produces are
loggers already (the parent exists before the child), so the hierarchy costs no
extra slot.

## Design

### D1 — One implementation, in `nros-log`

Per RFC-0019, Rust implements and C/C++ delegate.

- **Rust:** `Logger::create_child(&'static self, child_name: impl Borrow<str>)
  -> Result<&'static Logger, ChildError>` — rclrs 0.7's name and parameter
  shape (`create_child(&self, child_name: impl Borrow<str>) -> Result<Logger,
  RclrsError>`, read from the copy vendored under `play_launch`), per RFC-0089's
  same-name-same-shape rule. `Borrow` is in `core`, so the parameter costs
  nothing on `no_std`. Builds `"{parent}.{child_name}"` in a stack buffer, then
  `get_or_create_logger`. Idempotent: the same name returns the same logger, so a
  call inside a log statement costs one intern-table lookup.
  - **Fallible, as upstream is.** `ChildError` is `NameTooLong { len, max }` or
    `ArenaFull`. A Rust caller chooses its own fallback
    (`parent.create_child("x").unwrap_or(parent)`); D3's automatic fallback is
    the C/C++ behaviour, because rclcpp's `get_child` has no error channel.
  - **`&'static Logger`, not a value.** Runtime loggers live in the static arena
    and are never freed; this is the one shape difference, and it makes the row
    adopt-bounded rather than adopt.
  - **The catch-all's children are top-level names.** rclrs names the child of
    its default logger plain `child_name`, because that logger's name is empty.
    Ours is `"nros"`, and it plays the same role, so `DEFAULT_LOGGER.create_child("x")`
    is named `x`, not `nros.x`. The same rule holds in C and C++.
- **C:** `nros_logger_get_child(const void* logger, const char* suffix)`. Returns the
  child's handle, or NULL when no child logger could be created (D3).
- **C++:** `rclcpp::Logger::get_child(const std::string&)` (hosted) and
  `get_child(const char*)` (both). Returns a `Logger` holding the full requested
  name in its owned `FixedString` and the child's handle.

### D2 — Inheritance is a parent pointer, resolved at creation

`nros_log::Logger` gains `parent: AtomicPtr<Logger>`; null for a root, for every
`Logger::new` / `const` logger, and for the catch-all.

- **Effective level:** own level if set; else walk `parent` until a level is set;
  else `Logger::default_level()`. The walk is pointer loads only — no string work
  on the log path — and is bounded: a parent is always created before its child,
  so the chain is acyclic and no longer than `MAX_LOGGERS`.
- **Linking at creation:** creating a dotted name links it to its NEAREST EXISTING
  ancestor (trim at the last `.` and look up, repeatedly). Creating any logger also
  re-points existing loggers whose current parent is a FARTHER ancestor of theirs
  than the new one — one scan of at most `MAX_LOGGERS` slots, at creation only. The
  result is rcutils's rule for every name that exists as a logger, independent of
  creation order. Setting a level by name already creates the logger
  (`nros_log_get_logger`), so a level can always be attached where upstream would
  attach one.
- **Cost:** one pointer per logger, including static ones; one or more extra
  atomic loads on `is_enabled` for a logger with no level of its own.

### D3 — A name over the limit: emit through the parent, refuse `set_level`

Runtime names are capped at `MAX_LOGGER_NAME_LEN` = 48 bytes per name. Node names
may be 64 bytes, and children nest (`controller_server.local_costmap.inflation_layer`
is already 47), so the cap is reachable. Today `resolve_logger` falls back to the
catch-all `"nros"` logger, whose level is SHARED by every unnamed logger; for a
child that would lose the subsystem AND let `child.set_level()` move every unnamed
logger's threshold — issue 1019's aliasing.

Decision, for a child that cannot be created (name too long, or arena full — D5):

- **Emit through the parent.** Records go out under the parent's name and are
  filtered at the parent's effective level, so they stay attributed to the right
  node.
- **Refuse `set_level`.** The child carries no level handle of its own;
  `set_level` returns an error, never redirects. This is the existing rule for a
  `rclcpp::Logger` built without a handle (`logger_names_and_levels_runtime.cpp`,
  "set_level on a Logger with no handle must REFUSE").
- **Warn once per process**, naming the name that did not fit and the slot/arena
  counts, as `resolve_logger` already does.
- C++ `get_name()` still returns the requested full name (it is the caller's
  string, owned since issue 1682). The record's logger name is the parent's; the
  divergence is stated on `get_child`.

The C++ `Logger` therefore separates its EMIT handle (parent on fallback) from its
LEVEL handle (null on fallback). The `/rosout` sink clips names at the same 48
bytes (`rosout::NAME_CAP`); the two move together if the cap ever changes.

### D4 — `/rosout` follows the configured ROS release

Upstream differs by release only here:

| | Humble | Iron, Jazzy |
| --- | --- | --- |
| `get_child` | names only | also `rcl_logging_rosout_add_sublogger(parent, suffix)` |
| child records on `/rosout` | **no** — `rcl_logging_rosout_output_handler` publishes only through "a rosout publisher correlated with the logger name … If there is no publisher directly correlated with the logger then nothing will be done", and only node loggers have one | **yes**, through the parent node's publisher, under `parent.suffix` |
| stops | n/a | when the last copy of the child is destroyed (a deleter calls `rcl_logging_rosout_remove_sublogger`) |

`nros_log::rosout::RosoutSink` today publishes EVERY record, which already
diverges from Humble for a free `get_logger("x")` (no node publisher correlates
with it), independent of children.

Decision: the sink filters by the release the image is built for (`ros-humble` /
`ros-iron` / `ros-jazzy` on `nros-node`; nros-log has none, so nros-node hands the
policy over at initialisation):

- A logger is marked as a NODE logger when `Node::logger()` resolves it.
- **Humble:** publish a record iff its logger is a node logger.
- **Iron, Jazzy:** publish iff its logger, or an ancestor through `parent` (D2), is
  a node logger.
- Free loggers are published by neither — matching both releases, and a change
  from today.

Bound: on Iron and later a child stays published for the rest of the image's
life. Upstream stops when the last copy dies; ours cannot without making every
C++ `Logger` copy reference-counted, which a log call would pay for. Runtime
loggers are never freed, so "registered until reset" is the honest shape.

### D5 — Arena size is a per-board default, and its use is reported

`dynamic-loggers-<N>` (0, 8, 16, 32; default 16). Measured 2026-10-05: no image in
the tree selects one, so a 64 KB MCU and a Linux host both get 16. Every node
takes one slot for its own logger, every distinct `get_logger("x")` one, and every
distinct child name one; three nodes with four children each is fifteen.

Cost per slot on a 32-bit target: ~16 bytes of `Logger` (with D2's pointer) plus
~24 bytes of name arena — 16 slots ≈ 0.6 KB of `.bss`, 32 ≈ 1.3 KB.

Decision:

- The default follows the BOARD: Linux host boards 32, MCU boards 16, overridable
  per image. Sizing from declarations (RFC-0100) is not possible here — `get_child`
  is a runtime call no declaration names.
- **Mechanism: a knob, not a feature.** The count becomes
  `NROS_LOG_DYNAMIC_LOGGERS`, resolved on the ladder every other pool count uses
  (RFC-0049; `knob()` in `nros-params/build.rs`): image env (`[image.<id>] env`) >
  Kconfig / board knob (`[board.knobs.*]`) > built-in 16. It reaches all three
  roads the way existing knobs do, with a `CONFIG_NROS_LOG_DYNAMIC_LOGGERS` pairing
  on Zephyr, and passes the knob-forwarding gates.
- **Why not the feature.** Today's `dynamic-loggers-{0,8,32}` features are
  unified across the graph, and `dynamic_logger_capacity` tests `-0`, then `-8`,
  then `-32`, so two crates selecting different sizes silently get the SMALLEST.
  A `compile_error!` on conflict would make that loud but still cannot say "the
  image overrides the board": features have union, not precedence. An env value
  has exactly one value per build, so the single-owner rule holds by
  construction. The features retire after one release with a deprecation note.
- `dynamic_loggers_in_use()` / capacity are reported by the boot report and by
  `just mem-report`, so the size is set from a measurement.

Exhaustion takes D3's fallback.

## Alternatives considered

- **Name-only `get_child`** (no inheritance). Rejected: compiles and drops records
  upstream prints (§Motivation).
- **Resolve ancestry by name on every log call** (trim at dots, look up). Correct
  without stored links, but string work on every `is_enabled`. Rejected for the
  hot path; D2 does the same walk once, at creation.
- **Snapshot the parent's level into the child at creation.** Wrong as soon as the
  parent's level moves afterwards.
- **Catch-all on overflow** (today's `resolve_logger`). Rejected: loses
  attribution and aliases a shared threshold (D3).
- **Parent on overflow, `set_level` forwarded to the parent.** Narrower aliasing,
  same defect.
- **`Logger::child` returning `Option`.** A new name for an upstream method, and
  a channel that cannot say WHY creation failed. Replaced by rclrs's spelling.
- **Keep the size a cargo feature, with `compile_error!` on conflict.** Loud,
  but cannot express an image overriding its board (D5).
- **Raise the 48-byte cap.** Moves the edge; every image pays the arena bytes. Not
  excluded later, as a knob, alongside `rosout::NAME_CAP`.
- **Always publish children on `/rosout`** (today's sink). Simpler and closer to
  newer releases, but a Humble image would publish records rclcpp drops.
- **Reference-count C++ `Logger` copies to unregister on last drop** (Iron's exact
  shape). Cost on every log call for a difference observable only after the last
  copy is gone.

## Open questions

None. The Rust spelling (D1) and the D5 mechanism were settled 2026-10-05.

## Changelog

- 2026-10 — created. Decisions D3 (parent + refuse), D4 (follow the release), D5
  (per-board default + reporting) agreed 2026-10-05.
- 2026-10 — open questions closed: D1's Rust surface is rclrs's
  `create_child(impl Borrow<str>) -> Result<&'static Logger, ChildError>`, with the
  catch-all's children as top-level names; D5's size is the
  `NROS_LOG_DYNAMIC_LOGGERS` knob on the RFC-0049 ladder, retiring the
  `dynamic-loggers-<N>` features.
