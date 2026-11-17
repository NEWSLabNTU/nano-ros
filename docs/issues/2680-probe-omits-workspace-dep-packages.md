---
id: 2680
title: "The C/C++ metadata probe adds a node package without the workspace packages it <depend>s on, so a node including a sibling package's header cannot be probed"
status: open
type: bug
severity: medium
area: [cli, cmake, build]
related: [phase-568, phase-313, phase-348, phase-367, issue-0662, issue-2280, issue-1578, issue-2608]
found: 2026-11-17
---

**GitHub:** #4149. **Home:** [phase-568](../roadmap/phase-568-build-args-and-profiles.md) W3, the
source probe's reach (W3a, #4131, gave the probe its workspace-wide cmake
args). **Owner:** nros 118's probe line, implemented under claim 2680.

## What

The probe is ONE generated CMake project per workspace
(`orchestration/metadata_probe_cmake.rs`, `render_probe_cmakelists`). It adds
each COMPONENT package with `add_subdirectory` and nothing else. A node package
whose sources include a header from a sibling workspace LIBRARY package (one it
names in `<depend>`) therefore cannot configure or compile in the probe, while
the real workspace build, which adds every package in `<depend>` order
(phase-348 W4, `nros dev ws order`), builds it fine.

Found in the Autoware safety island (`actuation_module`):
`controller_pkg` `<depend>`s on `asi_common` and `autoware_controller_libs`
(the latter at `src/autoware`, so directory and package name differ). With
the package's own fallback

```cmake
if(NOT TARGET autoware_controller_libs)
  add_subdirectory("${_ACT_ROOT}/src/autoware" autoware_controller_libs)
endif()
```

removed, `sync` fails with

```
probe FAILED for controller_pkg::controller: controller_node.hpp:18:10:
fatal error: autoware/trajectory_follower_base/control_horizon.hpp: No such file or directory
```

and the generated `build/nros-metadata/metadata-probe-cmake/CMakeLists.txt`
adds only `src/controller_pkg`. The fallback is the consumer working around the
probe: every node package with a workspace library dependency has to know that
the probe configures it alone.

Interface (message) dependencies are not affected: issue 0662 routes them
through `NROS_INTERFACE_SEARCH_PATH` + `nros_workspace_interfaces()`.

## Design shape (for review before implementation)

The probe stays one project per workspace (#4131's ruling). The dependency
packages are added to THAT project, before the members that need them.

1. **Which packages.** For each member package, the transitive closure of its
   workspace dependencies over the BUILD-time tags only: `<depend>`,
   `<build_depend>`, `<build_export_depend>`. `<exec_depend>` and
   `<test_depend>` are not followed (a bringup's `<exec_depend>` on a node must
   not pull it in). Resolved from `package.xml` by the existing scan,
   `provider_scan::scan_workspace_packages`, which gains a tag-aware
   build-dependency set (today `depends` merges every `*_depend`). Names are
   matched on `<name>`, never on the directory.
2. **Filtered out of the closure:**
   - names not in the workspace (`std_msgs`, Eigen, Autoware installs): found
     by `find_package` / the prefix path as today, not added;
   - interface packages (`interface_package::is_interface_package`): already
     served by the Find-stubs; adding their verbatim `ament_cmake`
     CMakeLists would fail without ROS and double-generate;
   - packages with no `CMakeLists.txt` (cargo-only, `ament_python`): nothing to
     add;
   - packages that are themselves members: added once, as members.
   The closure still WALKS through a filtered package's own deps, so a library
   behind an interface package is not lost.
3. **Order.** Members plus their union of dependency packages, ordered by
   `provider_scan::topological_order_with_priority` over the workspace scan,
   the order the real workspace build gets from `nros dev ws order`.
   Dependencies precede dependents; ties keep the current member order, so a
   workspace with no workspace library deps renders a byte-identical
   CMakeLists (no reconfigure, no re-probe on upgrade). Each package is added
   once, `add_subdirectory(<dir> <package name>)`.
4. **Cycles.** A cycle among the added packages refuses the probe with the
   packages on it, the same text `ws order` uses. No partial order.
5. **A dependency west application** (its CMakeLists calls
   `find_package(Zephyr)`, issue 2280's rule) cannot configure on the host:
   every member whose closure contains it is reported unprobeable, naming the
   dependency, before configure.
6. **Configure blame (phase-367 W4).** A `CMake Error at <dep>/CMakeLists.txt`
   blames every member whose closure contains `<dep>`; the retry re-renders
   with those members dropped and any dependency no remaining member needs
   dropped with them.
7. **Freshness (issue 1578's key).** `probe_inputs_key` gains the member's
   resolved dependency set: sorted `name=source_digest(dir)` over the closure.
   Editing a dependency's header, or adding or removing a `<depend>` on a
   workspace package, re-probes that member. The field is appended only when
   the closure is non-empty, so existing sidecars of dependency-free members
   stay fresh. The project-level change (a different added set) already
   reconfigures through the write-if-changed CMakeLists.
8. **Consumer fallbacks keep working.** A package's own
   `if(NOT TARGET x) add_subdirectory(...)` sees the target already defined and
   does nothing; it can then be deleted.

## Test

- Fixture workspace: a library package (a header + `INTERFACE` or `STATIC`
  target, no node) and a node package that `<depend>`s on it and includes its
  header, with no fallback. `nros configure` under `NROS_PROBE_FAILURE=refuse`
  passes; on main it fails with the `No such file or directory` above.
- Unit tests on the render: dependency before member; directory name differs
  from package name; interface, external and cargo-only deps not added;
  `<exec_depend>` not followed; a transitive dep through an interface package
  kept; a member that is also a dep added once; a workspace without deps
  renders unchanged.
- A cycle refused naming its packages; a Zephyr dep refused naming it.
- The freshness key moves on a dependency header edit and on a `<depend>` edit,
  and does not move for a dependency-free member.
- `just check workspace-probes` stays 53/53.
