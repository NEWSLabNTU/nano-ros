---
id: 253
title: "C++ interface FFI crates are flat-module supersets — two interface pkgs on one link line duplicated every shared nros_cpp_* symbol"
status: open
type: bug
severity: medium
area: codegen-cmake
related: [issue-0052]
---

## Symptom

A C++ Node pkg depending on TWO interface packages (first real case: the
autoware-safety-island example's `autoware_control_msgs` + `tier4_system_msgs`,
where tier4 depends on control) failed the final link with hundreds of
`multiple definition of nros_cpp_{publish,serialize,deserialize}_*`.

## Cause

`nros_find_interfaces` generates pkgs in topo order and passes ALL preceding
pkgs as `DEPENDENCIES`; each pkg's FFI crate `include!()`s the rs closure of
every preceding pkg (flat module, so cross-package field types resolve). Every
crate is therefore a superset of the previous one, and any consumer linking
more than one interface lib gets the shared closure twice. Single-dep examples
(std_msgs-only talkers) never exercised this.

## Mitigation landed (this commit)

- `nros_generate_interfaces` gained `NO_FFI_CRATE`; `nros_find_interfaces`
  builds ONLY the topo-last pkg's crate and routes that one archive through
  every pkg's `<pkg>__nano_ros_cpp` INTERFACE target (same imported target on
  a link line de-dupes).
- Case-normalized the LANGUAGE compare (verbs pass lowercase `cpp` — the
  documented enum-ish-args pitfall, hit again here).

## Residual gap (why this stays open)

Two `nros_find_interfaces` CALLS in one build with different topo-last pkgs
(e.g. consumer A depends only `autoware_control_msgs`, consumer B adds
`tier4_system_msgs`, A's subdir configures first) still produce two superset
archives on B's link line. Proper fix: per-pkg crates containing ONLY their
own `#[no_mangle]` fns, with dep TYPES imported (crate deps or type-only
includes) instead of full-source `include!()` — then any combination links.

Also fixed alongside (same template family): C++ msg constants are now struct
members (`MrmBehaviorStatus::AVAILABLE`, rosidl convention) in msg/srv/action
templates; the namespace-level `<Msg>_<NAME>` aliases remain.

## Candidate follow-up

Zero-init generated C++ structs (`= {}` member initializers): rosidl C++
zero-initializes, nano-ros PODs don't — an Autoware port leaked stack garbage
into `ResponseStatus.code` over the wire (autoware-safety-island-example
porting-notes 09).

## Baseline finding (2026-07-24)

`examples/templates/local-msg-package` (consumer links `local_msgs` +
`extra_msgs` via the `nros_workspace_interfaces` route) already fails with 387
duplicate definitions ON MAIN BEFORE this change — the template is not
fixture-gated, so the rot was silent. The `nros_workspace_interfaces` route
calls `nros_generate_interfaces` per pkg directly and is NOT covered by the
`nros_find_interfaces` dedupe; it needs the same last-superset treatment (or
the proper per-pkg-symbols fix).
