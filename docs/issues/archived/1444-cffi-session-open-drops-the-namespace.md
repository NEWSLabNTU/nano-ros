---
id: 1444
title: "The CFFI session open drops `RmwConfig::namespace`, so every nano-ros
  image advertises its primary node at the ROOT whatever its namespace resolved
  to"
status: resolved
type: bug
area: rmw, boot
related: [rfc-0045, 0794, 1434]
resolved_in: 2026-09-22
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

## Resolution

Resolved 2026-09-22.

### Two halves, and either alone is unobservable

The issue named the outbound half. There was a second one, and finding it
mattered: fixing only the first changes nothing a test or a bus can see.

* **Outbound** — `CffiRmw::open` read every field of `RmwConfig` except
  `namespace`, so `CffiSession::namespace_buf` stayed zeroed. Threaded now
  through `open` / `open_with_properties` / `open_named` /
  `open_named_with_properties` / `open_marshalling_properties` into
  `open_with_vtable`, which writes the buffer before the session view
  borrows it. The parameter is added to every `open*` rather than bolted on
  as a second `*_ns` family: these are Rust functions with no pre-built
  caller, unlike the C-ABI runners issue 1434 had to make additive.
* **Inbound** — `create_session_trampoline` in `rust_adapter.rs` rebuilt the
  config for a Rust backend with `namespace: ""` HARDCODED, three lines
  under a `node_name` it read properly. It reads `(*out).namespace_` now —
  the storage `rmw_vtable.h` says the runtime hands over. Every in-tree
  zenoh build reaches a backend through this adapter, so the outbound half
  on its own would have moved nothing.

`CffiSession::namespace()` joins `node_name()`. Its absence is part of why
this lasted: the node name had a Rust reader and the namespace had only the
borrowed pointer a backend follows, so nothing on this side could ask what
the session's namespace was.

No header edit, no ABI change — as the issue said. The runtime was simply
not filling one of the two fields `rmw_vtable.h` promises `create_session`.

### Empty is passed through, not defaulted

`""` crosses the seam as `""`. The backends already read an empty namespace
as the root (`shim/session.rs`, "Treat empty namespace as root"), so a
second default here would be a second place for the two to disagree — the
shape issue 1015 measured one layer down.

### The blast radius, and what asserted the old behaviour

Every namespaced image's SESSION node moves from `/` to `/<ns>`. It is a
wire change, it is what rclcpp does, and the observable difference is that
`ros2 node list` on such an image printed TWO lines (the session at `/`,
the per-node entities where they belonged) and now prints one.

**Nothing asserted the old behaviour.** Audited:

* `workspace_features_e2e` `rust_remap` is the only namespaced image in
  tree (`/island`). Its proof is `Proof::RemapWireName` — topic names, which
  come off the node handle and do not move.
* `rust_multi_node_per_node_graph` asserts EXACT equality on
  `{/listener, /talker}`, and its own comment names the phantom `/node` this
  issue is about. That image is in the ROOT namespace, so this change moves
  nothing in it — the assertion is a live tripwire for the class, not a
  fossil of the bug, and it was left alone deliberately.
* The zenoh keyexpr tests in `shim/mod.rs` take an explicit namespace and
  never reach this seam.

So there was no assertion to classify as "fossil or contract". The one
thing that DID encode the bug was prose: `namespace_buf`'s own doc-comment,
"Empty for now — `RmwConfig` does not yet carry a namespace through the cffi
path; reserved for future use."

### Measured on the wire

`rmw_zenohd`, the same C probe shaped like the generated typed entry, one
fixed-shape binary per row so each symptom stays attributable:

| bake | env | node_create ns | `ros2 node list --no-daemon` |
| --- | --- | --- | --- |
| `/island` | — | `"/"` before | `/cprobe` |
| `/island` | — | `"/"` after | `/cprobe` + `/island/cprobe` |
| `/island` | — | `"/island"` (with 1443) | `/island/cprobe` |
| none | — | `"/"` after | `/cprobe` |
| `/island` | `NROS_NODE_NAMESPACE=/fromenv` | `"/"` after | `/cprobe` + `/fromenv/cprobe` |

Row 2 is this fix ALONE: the node is still created at the root — the
pre-1443 template shape, held constant on purpose — and the line that moved
is the session's own token. That is the exact mirror of 1443's measurement,
where the node moved and the session did not, which is what made the two
attributable. Row 5 is the RFC-0045 precedence rung: `$NROS_NODE_NAMESPACE`
still outranks the bake and now reaches the token. Row 4 is the negative
direction: nothing baked stays at the root and never becomes `""`.

### Regression cover

`packages/rmw/cffi/tests/rust_adapter.rs` — three tests driving the WHOLE
chain (`RmwConfig` → `CffiRmw::open_with_rmw` → the open family →
`namespace_buf` → the session view → `create_session_trampoline` →
`RmwConfig` → backend), because a test of one seam passes while the other
half is still broken. The negative case failed on its first run for a
different reason worth recording: `cargo test` runs a file's tests as
threads of one process, so the shared recording slot handed it another
test's open. It holds a lock now — `cargo nextest`, which gives each test
its own process, would have hidden that.
