---
id: 1003
title: "Every image built from a CMake entry template registers as `node` — ten
  templates drop the session name, so two processes hash to one XRCE client"
status: resolved
type: bug
area: codegen, api, rmw
severity: high
found: 2026-09-03
related: [issue-0968, issue-0794]
---

## What was measured

The generated entry for a zephyr C++ talker called:

```cpp
::nros::create_node(__nros_node, "talker");                       // node name: correct
::nros::board::ZephyrBoard::run_components(&__nros_entry_setup);  // session name: ABSENT
```

One argument, so it bound to the overload that supplies the default:

```
nros-cpp/include/nros/main.hpp
  template <typename Setup> static int32_t run_components(Setup&& setup) {
      return run_components(NROS_ENTRY_LOCATOR, "node", static_cast<Setup&&>(setup));
```

Every image built this way registered as `"node"`. The XRCE client key derives
from that name, so a talker and a listener hashed to ONE client and the agent
saw a single peer — exactly what the C++ pubsub cell's note in `zephyr.rs`
predicts: *"needs distinct XRCE session_names per cpp process (shared-key hash
collided as one client)"*. It is also the name `ros2 node list` shows.

**Ten templates, not one** — every board family in both variants:

```
zephyr_entry_main{,_c}_typed.cpp.in
native_entry_main{,_c}_typed.cpp.in
freertos_entry_main{,_c}_typed.cpp.in
nuttx_entry_main{,_c}_typed.cpp.in
threadx_entry_main{,_c}_typed.cpp.in
```

## Cause: two producers of one artifact, and only one got the fix

There are two entry generators:

* **`emit_cpp.rs`** — the CLI path (`nros build` workspaces). It passes
  `nros_boot_config_node_name(&NROS_BOOT_CONFIG)`, and has since `b506a1376`
  (2026-06-27, phase-266 W5/W6/W7 — *"C/C++ entries name the session from
  .nros_boot_config"*).
* **`cmake/templates/*_entry_main*_typed.cpp.in`** — the `nano_ros_add_node`
  path, which is what the example leaves and workspace packages actually use.
  These never got it; the zephyr template has not changed since 2026-06-13.

So the fix landed in one producer and the other kept the old behaviour, while a
reader of either the CLI emitter or the header would conclude it was fixed
everywhere. Same shape as issues 0978, 0984 and 0998 — one fact, two spellings.

## The signatures differ by board, which is the trap

Correcting all ten is not one edit repeated ten times:

| board | overloads |
| --- | --- |
| `LinuxBoard` | `(session_name, setup)`, `(setup)` |
| zephyr / freertos / nuttx / threadx | `(locator, session_name, setup)`, `(locator, setup)`, `(setup)` |

`LinuxBoard` has NO locator parameter and no 3-arg form. I applied the 3-arg
shape uniformly at first, and the build said so:

```
error: no matching function for call to
  'nros::board::LinuxBoard::run_components(const char [1], const char [9], int32_t (*)())'
```

Eight templates take `(locator, name, setup)`; the two native ones take
`(name, setup)`.

## Verified

`just build-test-fixtures lane=native` → `== native == OK`, EXIT=0, and the
entry that previously failed to compile now reads:

```cpp
::nros::board::LinuxBoard::run_components("listener", &__nros_entry_setup);
```

— the node's own name, where it previously defaulted to `"node"`.

## Two wrong filings before this one, kept because the pattern is the lesson

1. **"The C++ emitter never passes a session name."** Wrong producer:
   `emit_cpp.rs` does pass it. I read the generated file and attributed it to the
   generator I happened to look at.
2. **"The generated entry is a stale artifact."** Also wrong. Its mtime was six
   weeks old, but its CONTENT was byte-identical to the template, which had not
   changed since 2026-06-13. Old timestamp, current content.

The second retraction did real damage downstream: on its strength I declared
issue 0968's nine zephyr failures invalid as "measurements of stale images".
**That retraction is itself retracted** — those results measure current
behaviour and stand.

The transferable part: an old mtime is not evidence of stale content, and a
generated file tells you about its producer only once you have established WHICH
producer made it.

## Left open

No gate. The property is cheap to state — a template that calls
`run_components` without a session name is the defect, and it is visible in the
template text — but the right shape is not obvious, because the arity that
carries the name differs by board. A naive "must have N arguments" check would
be wrong for `LinuxBoard`.

## Acceptance

* [x] Every CMake entry template passes the node's name as the session name.
* [x] Each call matches its board's declared overload.
* [x] A regenerated entry carries the name, and the native lane builds.
* [ ] A gate that fails when a template drops it again.
