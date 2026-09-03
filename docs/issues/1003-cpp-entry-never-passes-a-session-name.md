---
id: 1003
title: "The generated C++ entry never passes a session name, so every C++ image
  registers with the XRCE agent as `node`"
status: open
type: bug
area: codegen, api, rmw
severity: medium
found: 2026-09-03
related: [issue-0968, issue-0794]
---

## What was measured

`nros codegen entry --lang cpp` emits, for the zephyr talker and listener
(`<ws>/build-cpp-{talker,listener}-xrce/nros-entry/zephyr_entry_main.cpp`):

```cpp
::nros::Result r = ::nros::create_node(__nros_node, "talker");     // node name: correct
...
::nros::board::ZephyrBoard::run_components(&__nros_entry_setup);   // session name: ABSENT
```

The NODE name is right. The SESSION name is a separate argument and is not
passed, so the call binds to a delegating overload:

```
packages/api/nros-cpp/include/nros/main.hpp:361
  /// Phase 266: delegates to 3-arg overload with "node" default session name.
  template <typename Setup> static int32_t run_components(const char* locator, Setup&& setup) {
      return run_components(locator, "node", static_cast<Setup&&>(setup));
```

So a C++ talker and a C++ listener both register as `"node"`.

## Why it matters

The XRCE client key is derived from the session name. Two processes presenting
the same name hash to the same key, and the agent treats them as ONE client. The
test suite already knows this — the C++ pubsub cell carries the note:

> needs distinct XRCE session_names per cpp process (shared-key hash collided as
> one client)

The session name is also what `ros2 node list` shows, so this is a wire-identity
defect, not only an XRCE one.

## The doc comment says it is already fixed

`main.hpp:330-331`, on the 3-arg overload:

> `session_name` sets the primary session / node name (`ros2 node list`).
> NULL or empty → `"node"`. **The generated C++ entry calls this with**
> `nros_boot_config_node_name(&NROS_BOOT_CONFIG)`.

The generated entry does not. Either the emitter regressed or the comment
described an intent that never shipped; which one is NOT established here.

Issue 0794 is the neighbouring half — the boot config's writer sets
`NODE_NAME` only when `plan.nodes.len() == 1` and leaves domain, locator and
namespace clear. If the emitter is fixed to pass
`nros_boot_config_node_name(...)`, 0794 decides whether that value is populated,
so the two want reading together.

## What this is NOT

It is **not** the cause of issue 0968's zephyr XRCE cluster. All nine cases
there fail — rust and C included — and neither goes through this path. This was
found while chasing that cluster and is recorded separately precisely so it is
not mistaken for the answer.

## Direction

1. Decide whether the emitter should pass `nros_boot_config_node_name(...)` (the
   comment's claim) or the node name it already has in hand at
   `create_node(..., "talker")`.
2. Whichever: a gate. The distinguishing property is cheap to check — a
   generated C++ entry that calls `run_components` with no session name is the
   defect, and it is visible in the emitted source.
3. Read with 0794, which decides what the boot-config value would be.

## Acceptance

* Two C++ images built from different nodes register with distinct session
  names.
* Something fails if a generated C++ entry drops the session name again.
