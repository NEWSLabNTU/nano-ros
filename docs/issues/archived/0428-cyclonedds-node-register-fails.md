---
id: 428
title: "every CycloneDDS runtime test fails at node registration — session opens, register does not"
status: resolved
type: bug
area: rmw
related: [issue-0422, issue-0095, issue-0413, phase-321]
resolved_in: issue-0413 + decl_err_from_node widening
---

Same class as #0413. The declarative Node API never registered Cyclone type
descriptors: the type-erased sink (`create_generic_publisher_with_qos`) had a type
NAME but no `M`, so `find_descriptor` returned null → `PublisherCreationFailed` →
`NodeDeclError::Runtime` → the opaque `NodeRegister("<pkg>")`. C/C++ (static
descriptor table) and zenoh/XRCE (no registry) were unaffected, which is the clean
split #428 observed. #0413 fixed it — `register_declared_type`/`_service`/`_action`
at the declarative builder sites register the descriptor at the last point that
still knows `M` (a no-op unless a descriptor-needing backend installed a registrar,
so zenoh/XRCE stay clean), plus `ad7752bc9` caught the action-half funnel.

VERIFIED resolved on current main (2026-08-05) with freshly-built `rmw-cyclonedds`
Rust examples: node registration no longer fails; the diagnostic eprintln below
stays silent; pubsub delivers (`I heard: [Hello World: 1..4]`) and the service pair
delivers (client `Result of add_two_ints: 5`, server `Incoming request a:2 b:3`).

Also landed the debuggability fix #428 asked for as its first step:
`decl_err_from_node` (`node_runtime.rs`) now surfaces the real `NodeError` variant
on the error path (std-gated `eprintln!`) instead of collapsing everything into an
opaque `NodeRegister`. That collapse is why this class was mis-diagnosed twice; the
seam now names WHAT failed, not just WHERE.
