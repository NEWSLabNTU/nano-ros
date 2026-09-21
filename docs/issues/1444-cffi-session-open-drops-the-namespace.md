---
id: 1444
title: "The CFFI session open drops `RmwConfig::namespace`, so every nano-ros
  image advertises its primary node at the ROOT whatever its namespace resolved
  to"
status: open
type: bug
area: rmw, boot
related: [rfc-0045, 0794, 1434]
---

## Problem

MEASURED on the wire, 2026-09-21, zenoh, three runs on three domains:

```
namespace baked /island, no env      @ros2_lv/77/<zid>/0/0/NN/%/%/probe
                                     @ros2_lv/77/<zid>/0/0/NN/%/%island/probe
nothing baked, no env                @ros2_lv/78/<zid>/0/0/NN/%/%/probe
                                     @ros2_lv/78/<zid>/0/0/NN/%/%/probe
baked /island, $NROS_NODE_NAMESPACE=/fromenv
                                     @ros2_lv/79/<zid>/0/0/NN/%/%/probe
                                     @ros2_lv/79/<zid>/0/0/NN/%/%fromenv/probe
```

The FIRST token in each pair is the session's own, declared inside
`Session::open` before the executor exists. It is at the root in all three —
including the env case, which predates issue 1434 — so `ros2 node list` shows a
namespaced image TWICE: once correctly and once at `/`.

The cause is not in zenoh, which does the right thing with what it is given
(`session.rs`: "Treat empty namespace as root"). It is one seam up, and the tree
already says so in a comment:

```rust
// packages/rmw/cffi/src/lib.rs
/// Borrowed-pointer storage for `namespace_`. Empty for now —
/// `RmwConfig` does not yet carry a namespace through the cffi
/// path; reserved for future use.
namespace_buf: [u8; NAME_BUF_LEN],
```

`Executor::open` fills `RmwConfig { namespace: config.namespace, .. }` and
`CffiRmw::open` never reads it: `open_with_vtable` takes
`(vtable, locator, mode, domain_id, node_name, options)` and no namespace at
all, so `namespace_buf` stays zeroed and the `NrosRmwSession` view hands the
backend an empty string.

## What is NOT the bug

The ABI has the field. `rmw_vtable.h` says the runtime supplies
`rmw_session_t` "with `node_name` / `namespace_` already filled", so no header
edit and no ABI change is needed — the runtime simply never fills one of the two
it promises to.

Nor is this issue 1434. That one made the launch-declared namespace reach
`ExecutorConfig`, which is what a post-link patcher and every per-node rung need.
This is the ONE consumer of `ExecutorConfig::namespace` that is reached before
any node exists.

## Why it stayed invisible

A namespaced image is rare in-tree, and the duplicate reads as an extra node
rather than as a wrong one — `ros2 node list` prints both lines and neither is
obviously the defect. Issue 1434's own acceptance probe found it only because it
compared the same image with the namespace set and clear.

## Fix shape and its blast radius

Thread `config.namespace` from `CffiRmw::open` through `open_with_vtable` /
`open_marshalling_properties` into `namespace_buf` before `create_session`, and
normalise empty to `"/"` at exactly one end (the backend already does it, so the
runtime should pass what it has and not a second default).

The radius is real and worth saying: today EVERY namespaced image advertises its
session node at `/`, and after the fix it advertises it at `/<ns>`. Any test
that greps `ros2 node list` for a bare name on a namespaced image changes
answer. That is the correct answer — it is what rclcpp does — but it is a wire
change, not a no-op.

## Acceptance

The same probe, same three runs: the session token must carry the resolved
namespace in the first two and `/fromenv` in the third, and an image with
nothing baked and nothing in the environment must still be at the root and never
at `""`.
