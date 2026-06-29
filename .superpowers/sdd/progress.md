# Phase 268 — launch node identity + per-node graph (SDD)
Task W1: complete (commit 153664953, RuntimeCtx.node_identity + macro inject + create_node override; override test passes; nros-platform recompile clean; stale-diagnostics false alarm)
Task W2: implemented (commit ce32d415e, per-node NN token lazy in zenoh shim + #104 gate). WORKS ONLY ON DIRECT/EMBEDDED SHIM PATH.
Task W3: BLOCKED (commit 960f46394, e2e tests added — correctly RED). Multi-node C++ + Rust still show only /node.
  ROOT CAUSE (verified by code trace, not theory): the RMW CFFI vtable drops per-entity node_name.
  - nros-rmw-cffi `create_publisher_trampoline` (rust_adapter.rs:437) derives node_name from the
    SESSION (`session_node_name(session)`), NOT from the incoming TopicInfo. The vtable fn-ptr sigs
    (lib.rs:458-519) carry topic_name/type/hash/domain/qos — NO node_name/namespace param. So
    TopicInfo.node_name set by nros-cpp/nros-node (correctly "talker"/"listener") never crosses to
    the backend; every entity inherits the session's single name ("node" for multi-node).
  - #98 single-node /talker worked because the session name == the one node's name. Multi-node opens
    the session generic "node" -> all entities tagged "node" -> W2 dedups to one "node" token =
    primary -> gate no-op -> only /node. EXACTLY the observed result, both languages.
  - cpp_robot_entry binary DOES contain W2 symbols (ensure_node_liveliness/per_node_liveliness) — not
    stale. (Rust native_entry fixture WAS stale, built 06-25 pre-W1/W2 — but rebuilding hits the same
    CFFI wall.)
  FIX REQUIRES extending the RMW vtable ABI (RFC-0035 frozen 34-slot, abi_version) to carry
  per-entity node_name (+namespace) across create_publisher/subscriber/service_server/service_client
  — caller (CffiRmw lib.rs) + 4 trampolines + every C/C++ backend impl + abi_version bump. The deep
  CFFI change W2 was scoped to avoid; contained-shim approach structurally insufficient for the
  hosted/CFFI path. DECISION POINT — surfaced to user 2026-06-29.
Task W2b: RESOLVED WITHOUT ABI CHANGE (in progress). Deeper trace found the fix is NOT a vtable ABI
  extension. `CffiSession::make_view()` builds a PER-CALL `NrosRmwSession` view; the create_* trampolines
  read node_name from THAT view (`session_node_name`). The caller already holds `topic.node_name` but
  built the view with the SESSION's name. Fix = thread the entity's node_name/namespace into the
  per-call view (new `entity_view` helper + 4 create_* sites in nros-rmw-cffi/src/lib.rs). No vtable
  signature change, no abi_version bump, no backend edits (Cyclone's publisher_create ignores
  node_name anyway; its graph is GUID-based). `cargo build -p nros-rmw-cffi` green. NEXT: rebuild
  cpp_robot_entry + rust native_entry fixtures (relink the fix), re-run W3 tests — expect /talker +
  /listener. User chose "design first"; design recorded here + this turn's narration.
