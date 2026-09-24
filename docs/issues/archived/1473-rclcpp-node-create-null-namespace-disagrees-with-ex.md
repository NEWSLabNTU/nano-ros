---
id: 1473
title: "`nros_cpp_node_create(ns = NULL)` forces the ROOT while
  `nros_cpp_node_create_ex(namespace_len = 0)` INHERITS the executor's
  namespace — one C++ FFI, two meanings for \"unset\""
status: resolved
type: bug
area: api
related: [rfc-0045, rfc-0089, 1434, 1443, 1456]
---

## Problem

`NodeBuilder::build()` in `nros-node` already implements "the caller named no
namespace ⇒ inherit the executor's":

```rust
// packages/core/nros-node/src/executor/node_record.rs
if let Some(ns) = self.namespace {
    ns_buf.push_str(ns)…
} else {
    ns_buf.push_str(self.executor.namespace.as_str())…
}
```

The two C++ FFI entry points reach that builder with opposite answers for the
same input:

* `nros_cpp_node_create_ex` leaves `.namespace()` UNCALLED when
  `opts.namespace_len == 0`, so an unset namespace INHERITS the executor's.
* `nros_cpp_node_create` substitutes the literal `"/"` for a NULL `namespace`
  and then always calls `.namespace("/")`, so an unset namespace OVERRIDES the
  executor's with the root.

Both are documented as "NULL / unset", and they are one FFI over one builder.

## Why it is filed separately from 1456

Issue 1456 is fixed and did not need this settled: the launch identity now
reaches an `rclcpp`-shape component through its `NodeHandle`, which carries the
node's OWN namespace, so nothing in the generated-entry path depends on
inheritance either way. Making `nros_cpp_node_create`'s NULL inherit was
weighed as a candidate fix for 1456 and rejected on its merits — it reaches the
namespace only, not the name, and the executor's namespace is the launch
namespace only for a SINGLE-node plan (`boot_config_view` emits identity facts
only when `plan.nodes.len() == 1`), so a multi-node plan's executor sits at the
root and a component in one would still be misplaced. RFC-0089's 2026-09-24
section records that reasoning.

What is left is the inconsistency itself, which is about the C ABI's contract
rather than about launch: a caller reading both signatures cannot predict which
namespace a node lands in, and the difference is invisible until an executor is
opened at a non-root namespace (which `nros::init(…, node_namespace)` has been
able to do since issue 1434).

## Not measured  *(the original note, kept)*

Read off `packages/api/nros-cpp/src/lib.rs` (`nros_cpp_node_create` ~line 2076,
`nros_cpp_node_create_ex` ~line 2226) and `node_record.rs`'s `build()`. No
image was built that opens a namespaced executor and then calls the 4-arg form
with NULL; the claim to hold is "the two functions compute different
namespaces for the same stated input", which the source states.

## Direction

Decide which meaning "unset" has at this ABI and make both spellings agree —
then say so where RFC-0045 states the unset-versus-root distinction. Whichever
is chosen, the 4-arg form's doc-comment (`"namespace — … or NULL for \"/\""`)
or the `_ex` form's has to move with it.

## MEASURED, 2026-09-25 — the claim holds at the builder and does NOT hold at the surface

Two layers, and the issue named only one of them.

**At the builder** — `nros-node`, mock session, `set_node_identity("session",
"/island")`, verbatim:

```
[measure 1473] executor opened with namespace = "/island"
[measure 1473] node_builder("unset").build()                -> record.namespace = "/island"
[measure 1473] node_builder("root").namespace("/").build()  -> record.namespace = "/"
[measure 1473] node_builder("empty").namespace("").build()  -> record.namespace = ""
```

The divergence as filed is REAL: those are the two answers the two entry points
compute for the same stated input.

**At the C++ surface** — a hand-built image linking `libnros_cpp.a` and the
shared C stub RMW backend, `nros_cpp_init_rmw(stub, NULL, 0, "i1473",
"/island", storage)`, a relative topic `chatter` read back through
`nros_stub_rmw_last_entity_name()`, verbatim:

```
executor opened at namespace "/island"
  nros_cpp_node_create("alpha", ns = NULL) -> 0
[probe] 4-arg, ns = NULL                     handle_ns=/          fqn=/alpha                  relative 'chatter' -> /chatter
  nros_cpp_node_create_ex("beta", namespace_len = 0) -> 0
[probe] _ex, namespace_len = 0               handle_ns=/          fqn=/beta                   relative 'chatter' -> /chatter
  nros_cpp_node_create("gamma", ns = "/") -> 0
[probe] 4-arg, ns = "/" (explicit root)      handle_ns=/          fqn=/gamma                  relative 'chatter' -> /chatter
  nros_cpp_node_create_ex("delta", namespace = "/") -> 0
[probe] _ex, namespace = "/" (explicit root) handle_ns=/          fqn=/delta                  relative 'chatter' -> /chatter
```

**All four agree — at the root.** So the consequence the issue predicted ("a
caller cannot predict which namespace a node lands in") is not what a C++
caller saw. What was actually happening is worse and unnamed: `_ex`'s
inheritance landed in the executor's `NodeRecord` and was then OVERWRITTEN on
the handle with `"/"` — `_ex` disagreed with itself. The handle is what
`resolve_node_entity_name` reads for every C++ publisher, subscription and
service, while `nros_node::executor::action` resolves `(name, namespace)` out
of `self.nodes[id]`, i.e. the RECORD. One node, two namespaces, split by entity
kind.

No router was needed: the stub backend gives a real executor and a real session
without touching a wire, and every `create_*` slot records the resolved name
before refusing (issue 1384).

### Caller sweep

`nros_cpp_node_create` — two callers. `node.hpp`'s `Node::create` (the only one
that can pass NULL, reached from `nros::create_node`, `create_node_on`,
`make_node`, `Executor::create_node` and `Node(NodeHandle, name, ns)`), and the
generated C entry `entry.c.jinja`, which renders `"{{ n.namespace|c_str }}"` —
an EXPLICIT string, `"/"` for a root node, and `emit_c.rs` asserts it never
emits `""`. Every one of the 13 `testdata/entry/c_*.golden` files writes `"/"`
or a real namespace. So **no generated caller depended on the NULL-forces-root
behaviour**; only a hand-written `main` or a component whose class names no
namespace can reach it.

`nros_cpp_node_create_ex` — `node.hpp`'s `NodeBuilder::build`, plus the census
fixture in `metadata_hooks` and its test twin. The generated C++ entry's
`configure` shape passes `n.namespace` explicitly (issue 1443) and its `rclcpp`
shape passes the launch halves on the `NodeHandle` (issue 1456); a multi-node
plan's executor sits at the root, where inherit and root are the same value.

## Resolution

**`NULL`, `""` and `namespace_len == 0` all mean UNSET — the node INHERITS the
executor's namespace. `"/"` means the ROOT, explicitly.** Both entry points
answer identically, and both now write the RESOLVED namespace back onto the
handle (`store_recorded_namespace`, one helper called from both), so the
handle, the `NodeRecord` and `nros_cpp_node_get_namespace` are one answer and
the publisher/action split above is gone.

The argument, and the argument against it, are recorded in RFC-0089
§"Settled: at the C++ node-create ABI, an UNSET namespace INHERITS and `"/"` is
the ROOT (2026-09-25)". In short:

* The options struct already says "inherit" for every other field —
  `rmw_name_len == 0`, `locator_len == 0`, `domain_id_override ==
  NROS_CPP_DOMAIN_ID_INHERIT`. The namespace was the one field that would have
  had to read its own struct's stated vocabulary backwards.
* ROS 2's semantics: an unspecified namespace inherits the containing context.
  Under "unset = root", `nros::init(…, node_namespace)` (issue 1434) reaches the
  session's identity and nothing the image creates afterwards.
* A `const char*` CAN carry `Option<&str>` — `NULL`, `""` and `"/"` are three
  distinguishable values, and the root costs one character that every generated
  entry already writes. That is why "make the two spellings distinct" and "make
  them agree" were not in tension here: the distinct spellings already existed,
  and one entry point was spending `NULL` on a value it did not need.
* `nros-c` never had the substitution: `rclc_node_init_default` REFUSES a NULL
  namespace and documents `"/"` for the root. The collapse was the C++ 4-arg
  form's invention, not a tree-wide convention.

Against, stated: the tree DID say "`nullptr` means root" at this constructor,
so this is a decision and not a cleanup. What bounds it — the behaviour differs
only on an executor whose namespace is NOT the root; on a root executor the two
are the same value, so every image that never names a session namespace is
bit-identical. `nros_cpp_init`'s own `namespace` argument is unchanged: NULL
there is the root, because the session is the outermost context and inherits
from nothing.

### Both directions, after the fix (same probe, same backend)

```
executor opened at namespace "/island"
  nros_cpp_node_create("alpha", ns = NULL) -> 0
[probe] 4-arg, ns = NULL                     handle_ns=/island    fqn=/island/alpha           relative 'chatter' -> /island/chatter
  nros_cpp_node_create_ex("beta", namespace_len = 0) -> 0
[probe] _ex, namespace_len = 0               handle_ns=/island    fqn=/island/beta            relative 'chatter' -> /island/chatter
  nros_cpp_node_create("gamma", ns = "/") -> 0
[probe] 4-arg, ns = "/" (explicit root)      handle_ns=/          fqn=/gamma                  relative 'chatter' -> /chatter
  nros_cpp_node_create_ex("delta", namespace = "/") -> 0
[probe] _ex, namespace = "/" (explicit root) handle_ns=/          fqn=/delta                  relative 'chatter' -> /chatter
```

### Acceptance

* `packages/api/nros-cpp/tests/compile/node_unset_namespace_runtime.cpp` — a RUN
  on `just check cpp`, on a NON-ROOT executor because on a root one every shape
  of this defect passes. Asserts the handle namespace, the fully-qualified name
  and where a relative topic lands, for both entry points and in both
  directions, plus `""`. Negative controls MEASURED against it: the pre-fix
  handle write (`"/"` unconditional) → **8 failures**, every inheriting
  expectation, with all four explicit-root assertions still GREEN; keeping the
  fix but restoring the 4-arg form's `"/"` substitution → **5 failures**, all on
  the 4-argument form with `_ex`'s node green, which is the divergence as filed.
* `nros-node`'s
  `an_unnamed_namespace_inherits_the_executors_and_an_explicit_root_does_not` —
  pins the builder half, which was already correct and had no test, so nothing
  stopped a fix from landing at the wrong layer.
* Documentation moved with the behaviour, one meaning per spelling, on both
  entry points: `nros_cpp_node_create`, `nros_cpp_node_create_ex` and
  `nros_cpp_node_get_default_options` in `lib.rs` (regenerated into
  `nros_cpp_ffi.h`), `nros_cpp_node_get_namespace`, and `node.hpp`'s
  `Node::create` / `create_node` / `create_node_on` / `NodeBuilder::namespace_`
  / `Node(NodeHandle, …)`'s precedence paragraph, and `executor.hpp`'s
  `Executor::create_node`. RFC-0045's changelog and RFC-0089 carry the decision.
