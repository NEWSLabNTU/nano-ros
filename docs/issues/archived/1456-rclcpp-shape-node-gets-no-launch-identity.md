---
id: 1456
title: "An `rclcpp`-shape component gets NEITHER the launch name nor the launch
  namespace — the generated entry hands it only an executor handle, so the
  component names itself and the launch file's identity reaches nothing"
status: resolved
resolved_in: "RFC-0089 §Settled 2026-09-24; RFC-0044 §1 amendment"
type: bug
area: codegen, api
related: [rfc-0043, rfc-0089, 1172, 1434, 1443, 1473]
---

## Problem

Issue 1443 made both entry packs create each node at the Plan's namespace
instead of the literal `"/"`. That fixed the `c` and `configure` node shapes,
which are the ones the ENTRY constructs. The `rclcpp` shape constructs nothing:

```
packages/cli/nros-cli-core/src/codegen/entry/packs/entry/cpp/node_body.jinja
    ::nros::NodeHandle __h({% if n.tiered %}executor{% else %}::nros::global_handle(){% endif %});
    if (!__h.valid()) return static_cast<int32_t>(::nros::ErrorCode::NotInitialized);
    __nros_comp_{{ n.index }} = new (__nros_comp_buf_{{ n.index }}) ::{{ n.class }}(__h);
```

`n.name` appears in that arm exactly once — inside
`report_component_failure("{{ n.name | c_str }}", …)`, a diagnostic string.
`n.namespace` does not appear at all. The identity comes from the component's
own constructor, which a USER wrote:

```cpp
explicit Talker(nros::NodeHandle h) : nros::NodeWithTimers<1>(h, "talker") {}
```

and `nros::Node`'s third parameter defaults to `nullptr`, which
`nros_cpp_node_create` substitutes `"/"` for. So a launch file that says
`<node pkg="rclcpp_pkg" exec="rcl_one" name="alpha" namespace="/island">`
produces a node called whatever its C++ class hardcodes, in the root namespace.

Visible in the committed golden (`cpp_native_shapes.cpp.golden`, the `rcl_one`
row): `::rclcpp_pkg::RCL_ONE(__h)` — no name, no namespace.

## Why this is its own issue rather than part of 1443

1443 was a half-delivery: the value existed in the Plan, the call took it, and
a literal was passed instead. This is not that. The entry has no call to pass
it to, and the missing thing is BOTH halves of the identity, uniformly — so
whatever the fix is, it is a change to how an `rclcpp`-shape component is
handed its identity, not a corrected argument.

It is also a DIVERGENCE from upstream rather than an internal inconsistency,
which makes it a design question: ROS 2's own component container passes the
launch-declared name and namespace into a component through `NodeOptions`
(`--ros-args -r __node:=… -r __ns:=…`), so a launch file's identity is
authoritative there and the class's literal is a default. Here the class's
literal is authoritative and the launch file's is ignored. RFC-0089 records
that "construction is not identity" for the merge of `ComponentNode` into
`Node`, but nothing records this.

## MEASURED on the wire (2026-09-24) — and the cost is larger than filed

The paragraph below said the claim was read off the templates. It has since
been put on a bus, and the inference was right about the name and INCOMPLETE
about the consequence.

A hand-shaped copy of a generated single-node entry (`run_components("alpha",
"/island", setup)`) with BOTH node shapes in one image, one session, against a
real `rmw_zenohd`:

```
[probe] rclcpp-shape     get_fully_qualified_name -> "/rclcpp_class_name"
[probe] configure-shape  get_fully_qualified_name -> "/island/beta"
$ ros2 node list --no-daemon
/island/beta
/rclcpp_class_name
$ ros2 topic list --no-daemon
/island/probe_configure
/probe_rclcpp
```

The missing NAMESPACE is the part that bites hardest: every RELATIVE topic the
component declares resolves at the root, so an `rclcpp`-shape component and a
`configure`-shape component from the SAME launch file publish on different
topics. That is a delivery bug, not only a naming one.

After the fix, same probe, same router:

```
[probe] rclcpp-shape     get_fully_qualified_name -> "/island/alpha"
[probe] configure-shape  get_fully_qualified_name -> "/island/beta"
$ ros2 node list --no-daemon
/island/alpha
/island/beta
$ ros2 topic list --no-daemon
/island/probe_configure
/island/probe_rclcpp
```

### The original note, kept

Read off the templates, the goldens and the `nros::Node` constructors, not off
a bus. An `rclcpp`-shape component in a NAMESPACED launch file has no in-tree
fixture (`cpp_native_shapes` is a codegen golden, and the only namespaced
launch file in tree — `workspaces/features` `rust_remap`, `/island` — is a Rust
node). So the claim to hold is "the identity is not passed", which the
generated text states; what a real image then reports is inferred from
`nros_cpp_node_create`'s NULL-is-root substitution, which issues 1443 and 1444
did measure.

## Direction

Two shapes, and the choice is the issue:

1. **The entry passes it** — give the `rclcpp` arm a
   `::{{ n.class }}(__h, "{{ n.name|c_str }}", "{{ n.namespace|c_str }}")`
   form. Needs every such component to have that constructor, which is an
   API requirement on user code and therefore a break.
2. **The component asks for it** — publish the launch identity on the
   executor (it already reaches `ExecutorConfig` since issue 1434) and have
   `nros::Node`'s `NodeHandle` constructor consult it when the caller passes
   `nullptr`. No user-code change, but it makes `nullptr` mean "inherit"
   rather than "root" at that one constructor, which contradicts what its
   doc-comment and `nros_cpp_node_create` both say today — RFC-0045's
   unset-versus-root distinction, one layer up.

Whichever is chosen, say so in RFC-0089 or RFC-0043: the current behaviour is
defensible and it is undocumented, which is the part that is not.

## RESOLVED — neither of the two; a third, and it is upstream's own mechanism

Recorded in RFC-0089 §"Settled: the LAUNCH FILE is authoritative over a
component's own name and namespace, and the `NodeHandle` is where it says so
(2026-09-24)", which carries the measurement above and the reasoning.

**`nros::NodeHandle` carries the launch-declared identity, and
`Node(NodeHandle, name, ns)` prefers it over its own arguments.** The handle is
the one value the entry hands a component, so it is this API's `NodeOptions`
seam — upstream passes the same facts into a component through
`rclcpp::NodeOptions` (`-r __node:=… -r __ns:=…`), where the remap outranks the
class's literal. **No user component's signature changes**: `explicit
Comp(nros::NodeHandle)` is still the one shape.

Precedence, per HALF, stated rather than emergent:

> launch-declared (on the handle) > the constructor's argument (the class's
> literal) > the type's default

A class that hardcodes a DIFFERENT name than the launch file declares is named
by the launch file — the only answer under which one component class can be
instantiated twice in one launch file. The halves are independent, so a launch
file declaring only a namespace leaves the class's name standing.

Negative direction: a node the launch file does not name gets `nullptr` on both
slots and its class's literal stands. The emitter reads the RAW plan values for
this, NOT the resolved `n.name` (which falls back to the node's `exec`, and
would therefore outrank every class literal in every launch file that omits
`name=`); `""` is treated as unset on both sides.

Direction 2 was rejected on measurement, not taste: the executor's namespace is
the launch namespace only for a SINGLE-node plan (`boot_config_view` emits
identity facts only when `plan.nodes.len() == 1`), it reaches the namespace and
not the name, and it would give `nullptr` a second meaning at one C entry point
while `nros_cpp_node_create_ex` keeps the first. That last disagreement is real
and now has its own issue, **1473**.

Acceptance: `packages/api/nros-cpp/tests/compile/node_launch_identity_runtime.cpp`
(a RUN on `just check cpp`, with both negative controls measured), the
`cpp_native_shapes` golden — which now carries a node declaring both halves
beside one declaring neither — and the before/after above.
