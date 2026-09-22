---
id: 1443
title: "A generated C or C++ entry creates every node at the ROOT — `\"/\"` is a
  literal in the template, while the Plan carries each node's namespace"
status: resolved
type: bug
area: codegen, boot
related: [rfc-0045, 1172, 1434]
resolved_in: 2026-09-22
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

## Measured, not inferred

2026-09-21, on the wire: a C image shaped exactly like the generated typed entry
(`.nros_boot_config` with `NROS_BOOT_SET_NAMESPACE` and `namespace_ = "/island"`,
then `nros_board_native_run_components_named_ns(name, ns, setup)`, then
`nros_cpp_node_create(executor, "cprobe", NULL, &node)` and a publisher on
`chatter`) linked against `libnros_cpp.a` and run against `rmw_zenohd` reports:

```
$ ros2 node list --no-daemon      /cprobe
$ ros2 topic list --no-daemon     /chatter
```

`/cprobe`, not `/island/cprobe`. NULL is not "inherit" at this edge either:
`nros_cpp_node_create` substitutes `"/"` for a NULL namespace, and
`nros_cpp_publisher_create` reads the namespace off the NODE HANDLE, never off
the executor. So on the C and C++ roads `ExecutorConfig::namespace` reaches no
wire-visible consumer at all: the per-node one is this issue, and the session's
own liveliness token is issue 1444.

## Acceptance

A C and a C++ image whose launch declares `<node name="talker" namespace="/island">`
create their node at `/island/talker`, asserted on the ON-WIRE name and compared
against the Rust image from the same bringup. Include the empty case: a node the
model gives no namespace must still reach the root, not `""` — RFC-0045's
distinction between "unset" and "configured to nothing", which the C edge cannot
express in a `const char*` and so has to normalise at one end.

## Resolution

Resolved 2026-09-22.

### The fix

One derivation, `codegen::entry::node_namespace`, and both entry packs pass
its answer to `nros_cpp_node_create` / `create_node` / `create_node_on`.

The literal `"/"` was not the whole class. The same question —
"what namespace does this plan node have?" — already had three authored
copies in `entry/mod.rs` (`tier_group_keys`'s `ns_of`, `sched_view`'s
`node_ns`, the `node_binds` inline) plus `emit_cpp::plan_node_fqn`'s own
four-arm match, and the two template literals made six answers to one
question. They agreed by luck; the literals were the pair that did not.
All of them read `node_namespace` now, so a bind can no longer name a
namespace its node was not created with, and C and C++ cannot grow separate
rules.

`None` and `Some("")` collapse to `"/"` there. That is the RFC-0045 half
this issue's acceptance asked for: a `const char*` edge cannot express the
difference between "unset" and "configured to nothing", so it is normalised
at the producing end — the same choice `nros_board_native_run_components_named_ns`
makes for the session rung and `plan_node_fqn` already made for contract-row
keys.

### Goldens

37 files moved. 36 only gain an explicit `"/"` where the C++ default
argument used to supply it — the same bytes reaching the compiler, now
written down. TWO change behaviour, the only rows whose plan namespaces a
node (`c_native_rich`, `cpp_native_rich`, both `/demo`):

```
-  nros_cpp_node_create(executor, "renamed_talker", "/",     &__nros_node_0);
+  nros_cpp_node_create(executor, "renamed_talker", "/demo", &__nros_node_0);
-  ::nros::create_node(__nros_node_0, "renamed_talker");
+  ::nros::create_node(__nros_node_0, "renamed_talker", "/demo");
```

### Measured on the wire

`rmw_zenohd`, a C image shaped exactly like the generated typed entry —
`.nros_boot_config` with `NROS_BOOT_SET_NAMESPACE` and
`namespace_ = "/island"`, `nros_board_native_run_components_named_ns`, then
the pack's own `nros_cpp_node_create` line — linked against
`libnros_cpp.a`. One binary per row, the node-create argument being the
only thing that differs:

| `nros_cpp_node_create` ns | `ros2 node list --no-daemon` | `ros2 topic list` |
| --- | --- | --- |
| `"/"` (before) | `/cprobe` | `/chatter` |
| `"/island"` (now) | `/island/cprobe` + `/cprobe` | `/island/chatter` |
| `"/"`, nothing baked | `/cprobe` | `/chatter` |

Row 3 is the negative direction, and it is the shape the emitter still
renders for an un-namespaced node: the root, never `""`.

The residual `/cprobe` in row 2 is the SESSION's own liveliness token, not
this node — issue 1444, fixed in the next commit so the two symptoms stayed
separable. With both fixes the same image reports `/island/cprobe` alone.

### The boundary of what was measured

The emitter's output is asserted at the BYTE level
(`typed_emit_creates_each_node_at_its_plan_namespace` in both packs, plus
the two `rich` goldens); the wire behaviour of that output is measured with
a hand-built image whose node-create call is that same line. A generated
entry was not put on a bus end to end, because that needs a resolved
SystemModel and no committed one exists. The two halves meet at the exact
call, which is the seam this issue is about.

### Follow-up

Nothing blocking. Noted rather than fixed: a namespace longer than
`NROS_CPP_NAMESPACE_LEN` makes `nros_cpp_node_create` return
`INVALID_ARGUMENT`, which aborts setup and therefore boot — loud, not
silent. `boot_config_view` already refuses one over 63 bytes with a named
error on the session rung; the per-node rung has no such producer-side
check.
