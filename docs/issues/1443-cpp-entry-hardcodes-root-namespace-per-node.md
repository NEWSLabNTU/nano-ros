---
id: 1443
title: "A generated C or C++ entry creates every node at the ROOT — `\"/\"` is a
  literal in the template, while the Plan carries each node's namespace"
status: open
type: bug
area: codegen, boot
related: [rfc-0045, 1172, 1434]
---

## Problem

Issue 1434 says of the Rust road that "a per-node registration already carries
its own namespace": `nros::main!` emits `runtime.node_identity = Some(("remap_talker",
"/island"))` and `NodeRecord` creates the node with it (issue 1172). That
statement does NOT hold for the C and C++ roads, and 1434 did not check it.

Both entry packs write the root in as a LITERAL:

```
packages/cli/nros-cli-core/src/codegen/entry/packs/entry/c/entry.c.jinja
    nros_cpp_node_create(executor, "{{ n.name|c_str }}", "/", &__nros_node_{{ n.index }});
packages/cli/nros-cli-core/src/codegen/entry/packs/entry/cpp/node_body.jinja
    ::nros::Result r = ::nros::create_node(__nros_node_{{ n.index }}, "{{ n.name | c_str }}");
```

and `n.name` is the BARE name — `emit_cpp.rs` builds it as
`n.name.as_deref().unwrap_or(&n.exec)`. The C++ spelling reaches
`create_node(Node&, const char* name, const char* ns = nullptr)`, whose null is
the root.

The information is present and unused. `Plan`'s node already carries
`namespace`: `boot_config_view` reads `n.namespace` to fill the blob, and
`sched.node_binds` renders `b.namespace` beside `b.name` into
`nros_cpp_bind_node_name_sched`. So the same view that binds a node's scheduling
context by `(name, namespace)` creates that node at `(name, "/")`.

## What 1434 did and did not fix

1434 made the launch-declared namespace reach `nros_cpp_init`, so the PRIMARY
SESSION's identity — `Executor::set_node_identity`, the liveliness token, the
namespace relative topic names resolve against — is now correct on the C and C++
roads. A node created afterwards by `nros_cpp_node_create` with an explicit
`"/"` overrides that for itself. The two rungs are independent and only one of
them moved.

## Why this is a bug rather than a design choice

Nothing states it as one. The literal carries no comment, the IR has the field,
and the sibling call three lines away (`nros_cpp_bind_node_name_sched`) spends
it. A node under `/island` in a launch file therefore appears at `/talker` in a
C or C++ image and at `/island/talker` in the Rust image built from the SAME
launch file, which is a cross-language divergence no RFC records.

## Acceptance

A C and a C++ image whose launch declares `<node name="talker" namespace="/island">`
create their node at `/island/talker`, asserted on the ON-WIRE name and compared
against the Rust image from the same bringup. Include the empty case: a node the
model gives no namespace must still reach the root, not `""` — RFC-0045's
distinction between "unset" and "configured to nothing", which the C edge cannot
express in a `const char*` and so has to normalise at one end.
