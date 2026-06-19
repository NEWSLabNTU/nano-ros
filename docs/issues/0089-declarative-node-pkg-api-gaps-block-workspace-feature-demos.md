---
id: 89
title: Declarative Node-pkg API gaps block several phase-263 workspace feature demos
status: open
type: enhancement
area: core
related: [phase-263, rfc-0024, rfc-0044]
---

## Problem

Phase-263 extends `examples/workspaces/{rust,c,cpp,mixed}` to demonstrate the
everyday-ROS feature set *in the declarative Node-pkg shape* (`Node` +
`ExecutableNode` + `nros::node!` for Rust; the `configure(Node&)` component for
C/C++). A1 (services, Rust) shipped, but completing the rest surfaced that the
**declarative Node-pkg API does not yet ergonomically support several features** —
they exist only in the `[[bin]]` / manual-poll shape (`examples/native/*`). Faking
the demos would mislead, so they are blocked on these gaps.

## The gaps (found 2026-06-19 while implementing phase-263 A1/A2)

1. **Service / action CLIENT is `tick(TickCtx)`-only and undocumented.** A node
   cannot issue a blocking client call from `on_callback(CallbackCtx)` (the executor
   is mid-dispatch). The call API (`TickCtx::call_for_name`) lives on the separate
   per-spin `ExecutableNode::tick(&mut TickCtx)` hook, which had **0 usages** before
   A1. A1's `service_client_pkg` works by arming a flag in `on_callback` and calling
   in `tick`, but this two-surface pattern is undocumented and non-obvious — book +
   an example were missing. (Now demonstrated by A1; needs a book note.)

2. **No runtime parameter-VALUE read in the declarative callback path.**
   `NodeContext::declare_parameter*` declares a parameter (and the callback
   `.writes()`/effect binding records it for the planner), but **`CallbackCtx` /
   `TickCtx` expose no `get_parameter` / value accessor**, and `NodeParameter` is
   declare-only. A Node-pkg can declare a parameter but cannot read its configured
   value at runtime. Grep confirms 0 runtime parameter reads in any declarative node
   (core, tests, examples). **Blocks phase-263 A2 (parameters).**

3. **Lifecycle (managed nodes) not wired for the plain-cargo workspace shape.**
   `ExecutableNode` has no transition hooks (`on_configure`/`on_activate`/…), and
   `nros-macros` (`nros::main!`) does **no** lifecycle handling — the five REP-2002
   lifecycle services are registered only via `Executor::register_lifecycle_services()`
   (the `[[bin]]` executor shape) or the `codegen-system` bake (`generate.rs` adds
   `nros/lifecycle-services` from `plan.lifecycle`). So a `system.toml [lifecycle]`
   block has no effect on a plain-cargo `nros::main!` Entry — the starter workspaces'
   build mode. **Blocks phase-263 A3 (lifecycle) for the workspace (cargo) shape**;
   it works in the bake/`[[bin]]` paths. Either teach `nros::main!` to honour
   `[lifecycle]` (emit `register_lifecycle_services` + autostart), or document that
   lifecycle requires the bake build.

4. **C/C++ service-in-component is raw-CDR only.** `nros-cpp` offers
   `bind_service_raw<C, &C::method>` (manual request/response CDR + alignment) and a
   poll-style typed `Service<S>` (not executor-dispatched), but **no typed
   `bind_service<C, &C::method>`** for the `configure()` component shape. The C/C++
   starter service demo would be hand-rolled CDR — fragile + unrepresentative.
   **Makes phase-263 A1 for C/C++/mixed low-quality without an API add.**

5. **Logging sink not initialized by the workspace Entry.** A node logs via
   `nros_info!(&LOGGER, …)`, but the one-time sink init (`nros_log::init(
   sinks::default())`) is done only in the `[[bin]]` examples' `main`. Neither
   `nros::main!` nor the board crate inits a sink, and a board-AGNOSTIC Node pkg
   cannot pick the (board-specific) sink itself — so node logs go nowhere in the
   workspace shape. **Blocks phase-263 A5 (logging).** Fix: the board/Entry inits a
   default sink (native → stdout; embedded → its writer) at boot.

## Root pattern (2026-06-20)

`nros::main!` (the plain-cargo workspace Entry) is a **thin** macro: it parses the
launch, emits one `register` per `<node>`, and **resolves `[tiers]`** (it imports
`resolve_tiers`) — but it does **not** wire the other `system.toml`-declared runtime
config: lifecycle, log-sink init, parameter values, or the param/safety *services*.
Those are honoured only by the `codegen-system` **bake** (`generate.rs`) or the
`[[bin]]` executor shape. So the starter/showcase workspaces (which build via
`nros::main!`) can demo **pub/sub, timer, service server+client, scheduling tiers,
and safety (CRC, via cargo features)** — but the rest need either the bake build or a
matured macro.

**High-leverage fix:** teach `nros::main!` to honour the full `system.toml` capability
set it already partly reads (tiers) — lifecycle autostart + services, log-sink init,
parameter binding — mirroring the bake's `generate.rs`. One change unblocks A2, A3,
A5 (and the param/lifecycle halves of the showcase) at once.

## Direction

Mature the declarative Node-pkg API so the workspace examples can demonstrate these
features cleanly (prerequisite for the gated phase-263 waves):

- Surface a parameter-value read on `CallbackCtx`/`TickCtx` (e.g.
  `ctx.parameter::<T>(name)`), reading the launch/config-resolved value.
- Add a typed C++ `bind_service<C, &C::on_request>` (typed request/response via the
  generated bindings) for the component shape; same for the client.
- Document the `tick(TickCtx)` client/action surface + ship the A1 client as its
  reference (done in A1).

Until then, phase-263 should sequence the features the declarative API FULLY
supports first (pub/sub, timer, service-server, lifecycle, safety, tiers) and gate
parameters + the C/C++ client/service-typed demos behind this issue.
