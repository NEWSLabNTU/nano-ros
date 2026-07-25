# Phase 305: Ament-parity component registration (RFC-0057)

**Status:** 📋 Planned (2026-07-25).
**Implements:** RFC-0057. **Closes:** issue 0275; 0277 (UX half — the
union-closure engineering half stays open).
**Reference consumer:** simple-autoware-safety-island (porting-notes 02/08/12).

## The shape change

Before (today's fused macro — ASI's emergency-stop port):

```cmake
nano_ros_add_node(mrm_emergency_stop_operator
    CLASS  autoware_mrm_emergency_stop_operator::MrmEmergencyStopOperator  # L.4-flattened ns
    HEADER autoware/mrm_emergency_stop_operator/mrm_emergency_stop_operator_core.hpp
    SHAPE  rclcpp
    SOURCES src/mrm_emergency_stop_operator/mrm_emergency_stop_operator_core.cpp)

# manual topo-last superset-archive pick (porting-notes 08 / issue 0277)
if(TARGET tier4_system_msgs__nano_ros_cpp)
    target_link_libraries(
        autoware_mrm_emergency_stop_operator_mrm_emergency_stop_operator_component
        PRIVATE tier4_system_msgs__nano_ros_cpp)
endif()
```

After (upstream Autoware CMakeLists is the left column; the port is a
macro-name swap):

```cmake
# upstream (ament)                                  # nano-ros port
ament_auto_add_library(mrm_emergency_stop_operator  nano_ros_auto_add_library(mrm_emergency_stop_operator
  SHARED                                              STATIC
  src/mrm_.../mrm_..._core.cpp)                       src/mrm_.../mrm_..._core.cpp)

rclcpp_components_register_node(                    nros_components_register_node(
  mrm_emergency_stop_operator                         mrm_emergency_stop_operator
  PLUGIN "autoware::mrm_emergency_stop_operator      PLUGIN "autoware::mrm_emergency_stop_operator
          ::MrmEmergencyStopOperator"                        ::MrmEmergencyStopOperator"
  EXECUTABLE mrm_emergency_stop_operator)             EXECUTABLE mrm_emergency_stop_operator
                                                      HEADER autoware/mrm_emergency_stop_operator/mrm_emergency_stop_operator_core.hpp)
```

- namespace in the C++ sources: verbatim upstream (`autoware::…`) — L.4 gone
- interface libs: auto-wired by `nano_ros_auto_add_library` — the
  `if(TARGET …__nano_ros_cpp)` block disappears
- `HEADER` only because upstream's `…_core.hpp` layout doesn't match the
  class name; omitted where convention (`a::b::Class` → `a/b/Class.hpp`) holds
- `SHAPE` defaults to `rclcpp` (legacy `configure` shape = explicit opt-in)

Simple in-tree example (templates/multi-package-workspace), after:

```cmake
nano_ros_auto_add_library(pkg_cpp_listener STATIC Listener.cpp)
nros_components_register_node(pkg_cpp_listener
    PLUGIN "pkg_cpp_listener::Listener"
    EXECUTABLE pkg_cpp_listener
    TYPED)
```

## Work items

### W1 — core implementation

- **W1.1 registration-only path.** `nano_ros_node_register` gains an
  internal `EXISTING_TARGET <lib>` mode: skip `add_library`, attach
  registration (metadata row, entry glue, lint symbols) to the given
  target. All existing callers unchanged.
- **W1.2 `nros_components_register_node(target PLUGIN … EXECUTABLE …)`.**
  Thin public verb over W1.1: `PLUGIN`→CLASS, `EXECUTABLE`→NAME,
  optional `HEADER`/`SHAPE`(default rclcpp)/`TYPED`/`DEPLOY`/
  `CALLBACK_GROUPS`. Keyword-parity gate: a call using ONLY
  rclcpp_components keywords must parse.
- **W1.3 `nano_ros_auto_add_library(name STATIC <srcs…>)`.**
  `add_library` + `_nros_generate_declared_interfaces` + automatic link
  of the package's generated interface libs (routing the ONE topo-last
  superset archive internally — 0253/0277 mechanics stay hidden) +
  include-dir inheritance.
- **W1.4 pkg into metadata (RFC-0057 D2).** Register writes
  `pkg: "${PROJECT_NAME}"` into `nros-metadata.json components[]`;
  `codegen/entry/metadata.rs` keys on it, class-prefix split demoted to
  fallback (old metadata keeps working). L.4 FATAL deleted from
  `NanoRosNodeRegister.cmake`.
- **W1.5 lint retirement.** `lint_class_pkg_prefix` (check_workspace +
  bringup call site) retired; replacement check: `class` must be a
  qualified `::` name. Lint tests updated (nested accepted, unqualified
  rejected).
- **W1.6 unit tests.** metadata.rs explicit-pkg + fallback; cmake smoke
  (configure-only fixture) for both new verbs incl. the
  rclcpp-keywords-only call.

### W2 — nano-ros in-tree migration

- **W2.1 templates + book.** `examples/templates/*` (multi-package
  workspace, local-msg-package) to the new spelling; book pages +
  `docs/reference/c-api-cmake.md` show the split shape as THE shape;
  legacy spelling moved to a compat note.
- **W2.2 examples sweep.** ~99 example CMakeLists
  (`nano_ros_add_node`/`nano_ros_node_register` users) migrated
  mechanically per platform family; per-family build via the existing
  fixture lanes (`build-test-fixtures` — fixture-treadmill rule: full
  rebuild after the sweep, no incremental).
- **W2.3 packages/tests.** The ~10 non-example users
  (`packages/testing`, verification fixtures) migrated the same way.

### W3 — ASI reference migration (the verbatim proof)

- **W3.1 un-flatten.** The four ported pkgs
  (`autoware_mrm_emergency_stop_operator`, `…_comfortable_stop_operator`,
  `autoware_stop_mode_operator`, `autoware_mrm_handler`) restore
  upstream namespaces (`autoware::…`), delete the
  `// nano-ros port: namespace X → Y` scar comments, update
  `system.toml` `class` rows to the nested names.
- **W3.2 CMakeLists.** New verbs; delete every manual topo-last
  `if(TARGET …__nano_ros_cpp)` block.
- **W3.3 receipt.** Native + zephyr islands rebuild; full MRM demo
  (`just demo-all`, both islands) PASS. porting-notes 02/08/12 annotated
  resolved-by-RFC-0057; slides appendix friction table updated
  (02 → fixed).

### W4 — cleanup + retirement

- **W4.1 legacy compat.** `nano_ros_add_node` / bare
  `nano_ros_node_register` stay as forwarding spellings (per RFC-0057) —
  one-line `message(DEPRECATION …)` under a `NROS_WARN_LEGACY_VERBS=ON`
  opt-in only (no default noise while W2 consumers exist out-of-tree).
- **W4.2 dead code.** L.4 code paths, the prefix-lint helpers, and the
  `metadata.rs` fallback's "L.4 enforced by cmake" comments purged;
  fallback kept but re-documented as pre-0057-metadata compat.
- **W4.3 docs closure.** Issue 0275 → resolved (archived); 0277 updated
  (UX half closed, union-closure half remains); CLAUDE.md pitfall
  one-liner for the topo-last link block deleted (obsolete);
  RFC-0057 Draft → Stable.

## Acceptance

- rclcpp-keyword-only `nros_components_register_node` call configures and
  builds (W1.6 fixture).
- Grep gates: no `${PROJECT_NAME}::` prefix requirement in cmake/lints;
  zero `if(TARGET …__nano_ros_cpp)` blocks in examples or ASI.
- `just ci` green (full sweep — W2 touches every example fixture).
- ASI: verbatim `autoware::` namespaces, macro-name-swap CMakeLists, cold
  `just demo-all` PASS on native AND zephyr islands.

## Ordering / risk

W1 lands alone (purely additive; legacy paths untouched → CI-safe).
W2 is mechanical but wide — do it per platform family with fixture
rebuilds between (mtime-treadmill). W3 is the acceptance proof and can
start as soon as W1 is in. W4 last. The register/lint area overlaps
active parallel-agent work (212-series) — coordinate before W1.5.
