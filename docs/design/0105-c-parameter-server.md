# RFC-0105 — The C parameter server: one family, rclc's shape, bounded for an RTOS

**Status:** Draft (2026-10-11)

**Implemented by:** [phase-491](../roadmap/phase-491-c-parameter-server.md).
**Applies:** [RFC-0089](0089-ros2-api-adoption-and-the-compile-or-conform-rule.md)
(compile-or-conform) to the C parameter surface. **Governed by:** RFC-0019 /
RFC-0020 — the Rust store (`nros-params`) is the implementation and C delegates.
**Closes, when implemented:** issues
[0793](../issues/0793-c-params-declared-via-legacy-store-invisible-to-ros2.md) and
[0865](../issues/0865-no-example-registers-parameter-services.md), and the C half
of [1812](../issues/1812-param-services-require-alloc.md).
**Builds on:** the executor-owned store of
[phase-382](../roadmap/archived/phase-382-unified-parameter-store-caller-owned.md)
and the service plumbing of phase-426.

## Summary

The C API has three parameter families today, and none of them is both served
and shaped like rclc. This RFC replaces all three with ONE: rclc's
`rclc_parameter_server_t` API, spelled with rclc's own names, over the
executor's parameter store. The upstream header path
`<rclc_parameter/rclc_parameter.h>` is shipped, so ported rclc code compiles
unchanged when it stays inside the bounded envelope below and fails to compile
when it steps outside it. Every declaration is visible to `ros2 param` through
the six `rcl_interfaces` services, and every change is published on
`/parameter_events`.

## Motivation

Measured on main at `846a275e3e` (2026-10-11):

| Family | Shape | Served to `ros2 param`? | Callers |
| --- | --- | --- | --- |
| `nros_parameter_server_t` + `nros_parameter_declare_*` (+ deprecated `nros_param_*`) | rclc-like server object, caller-owned storage | **No.** It is a separate store that no service reads (issue 0793) | one compile test and the crate's own unit tests |
| `nros_executor_{declare,get,set}_param_*` (+ `_on(node)`) | executor-keyed, no server object, no arrays | Yes | `examples/native/c/parameters`, the metadata census, two gate scripts |
| `nros_cpp_get_param_*` (from C components) | C++ shim reached from C | Yes | `c_param_talker_pkg`, codegen `tests_c.rs` |

The upstream-shaped family is served by nothing, the served family has
upstream's semantics with none of its names, and C components use a third
family that belongs to C++. A porting user cannot carry rclc code over, and a
nano-ros user has to learn which of three families reaches the wire.

## What rclc actually does

Read from `ros2/rclc` `rclc_parameter/src/rclc_parameter/parameter_server.c`,
branches `humble` and `rolling`, on 2026-10-11. The two branches export the same
function set. They differ in one place: rolling adds
`set_parameters_atomically`, so `RCLC_EXECUTOR_PARAMETER_SERVER_HANDLES` is 5 on
humble and 6 on rolling.

**Where the callback fires.** `rclc_parameter_execute_callback(server, old, new)`
is called from:

| Path | `old` | `new` | Fires? |
| --- | --- | --- | --- |
| local `rclc_parameter_set_{bool,int,double}` | current | proposed | **yes**, before the write; `false` → `RCLC_PARAMETER_MODIFICATION_REJECTED` |
| service set of a declared parameter | current | proposed | yes, through the same setter |
| service set with type `NOT_SET` (delete) | current | `NULL` | yes, then `rclc_delete_parameter` |
| service set of an undeclared name, `allow_undeclared_parameters` | `NULL` | proposed | yes |
| local `rclc_add_parameter` | — | — | **no** |
| local `rclc_delete_parameter` | — | — | **no** |

**Re-entrancy.** While the callback runs, `on_callback` is set, and every
mutating call (`add`, `delete`, `set_*`, `set_parameter_read_only`,
`add_parameter_description`, `add_parameter_constraint_*`) returns
`RCLC_PARAMETER_DISABLED_ON_CALLBACK` (40004, sic). `get_*` is allowed.

**What rclc does not enforce.** Read-only is checked on the **service** path
only. A local `set_*` on a read-only parameter succeeds. Integer and double
ranges are stored and reported by `describe_parameters`, and never checked on
any path.

**Events.** With `notify_changed_over_dds` (default `true`) every add, local
set and delete publishes one `ParameterEvent` carrying exactly one parameter, on
`/parameter_events` with `rmw_qos_profile_parameter_events`. A rejected set
publishes nothing.

**Return codes.** `RCL_RET_ERROR` for an unknown name or a full server,
`RCLC_PARAMETER_TYPE_MISMATCH` (4002) on a typed set of the wrong type, and
`RCL_RET_INVALID_ARGUMENT` on a typed get of the wrong type.

## Decisions

### D1 — One family, rclc's names are the primary spelling

The parameter API is `rclc_*`, declared in
`include/rclc_parameter/rclc_parameter.h` (the upstream path). These are the real
symbols, not aliases of `nros_*` names. RFC-0089's alias rule still holds in the
other direction: where nano-ros needs a function rclc lacks, it is an extension
spelled `rclc_parameter_*` / `rclc_*_parameter_*` in the same header and listed
under §Extensions. No `nros_` spelling of a parameter function survives.

The three families in §Motivation are retired (D9).

### D2 — The server is a handle over the executor's store

`rclc_parameter_server_t` holds `{executor*, node_id, options, on_modification,
context, on_callback, quota_used}`. It holds no parameter storage. Every
parameter lives in the executor's store, keyed by `NodeKey(node_id)`, which is
the store the six services read. That is the whole fix for issue 0793: there is
no second store to declare into.

Its size is fixed and small. `RCLC_PARAMETER_SERVER_T_SIZE` is asserted
against the Rust layout the same way the other `nros-c` opaque handles are.

### D3 — Initialisation refuses a node that is not bound to an executor

The store belongs to the executor, so `rclc_parameter_server_init_default` and
`rclc_parameter_server_init_with_option` read `node->executor`. A node made with
`rclc_node_init_default` and never passed to `nros_executor_node_init` has
none. Both return `RCL_RET_NOT_INIT` and leave the server zero-initialised.

rclc allows the server to be initialised before the executor exists. Ported code
written in that order fails at run time with a named code at the first call, not
silently. The order a user must follow is documented in the header and the book.

`rclc_executor_add_parameter_server[_with_context]` checks that its `executor`
is the one the node is bound to and returns `RCL_RET_INVALID_ARGUMENT`
otherwise. It registers the six services and installs the callback.

### D4 — Options

| Field | Behaviour |
| --- | --- |
| `notify_changed_over_dds` | ADOPT. Default `true`, as upstream. Creates the `/parameter_events` publisher (D7). |
| `max_params` | ADOPT-BOUNDED. A per-node quota inside the store's build-time capacity, not an allocation size. `add_parameter` past it returns `RCL_RET_ERROR`, as rclc's full server does. A quota larger than the remaining build-time capacity is refused at init with `NROS_RET_FULL` and names `NROS_MAX_PARAMETERS`. (`RCL_RET_BAD_ALLOC` is deliberately undefined in `rcl_compat.h`: its remedy is "free and retry", ours is "rebuild larger".) |
| `allow_undeclared_parameters` | ADOPT. Sets the store's per-node flag. |
| `low_mem_mode` | ADOPT-BOUNDED. Accepted and changes nothing. The services already stream one parameter at a time, so the low-memory behaviour is always on; descriptions are kept because the store has room for them. |

### D5 — No allocator

Initialisation, declaration, set, delete and fini allocate nothing. The store is
the executor's carved region, sized from the image's declarations
(`[params] store` in the sizing descriptor, `NROS_MAX_PARAMETERS`). The upstream
header's "Allocates Memory: Yes" rows become "No". Issue 1812 removes the
remaining heap use in the services, so the whole path runs on an image with no
heap.

### D6 — The callback, and where it fires

`rclc_parameter_callback_t` is upstream's type:

```c
typedef bool (*rclc_parameter_callback_t)(
  const Parameter * old_param, const Parameter * new_param, void * context);
```

It fires exactly where rclc fires it (§What rclc actually does): local typed
sets, service sets, service deletes and undeclared service adds. Add and local
delete do not fire it. `on_callback` re-entrancy refusal is adopted with the
same code and value.

This needs the Rust store's `OnSetParameterFn` to carry the old value and to be
reachable from a local set. Today `set` runs no callbacks and only `apply` (the
service path) does. The store gains one entry point for an rclc local set; C++
and Rust callers keep their current semantics.

`Parameter`, `ParameterValue` and the sequence types are nano-ros structs with
the **member paths** of the `rcl_interfaces` C messages: `name.data`,
`name.size`, `value.type`, `value.bool_value`, `value.integer_value`,
`value.double_value`, `value.string_value.data`, and
`value.<x>_array_value.{data,size,capacity}`. Code that reads a parameter
through those paths compiles. They are not rosidl ABI types, and there are no
`rcl_interfaces__msg__Parameter__init/fini` functions, so code that builds its
own `Parameter` with the rosidl runtime fails to compile (REFUSE-LOUD). String
and array members point into the store and are valid only for the duration of
the callback.

### D7 — Read-only and ranges

**Service path (`ros2 param set`, set_atomically): read-only and ranges are
enforced.** Read-only matches rclc. Range enforcement is stricter than rclc and
matches rclcpp. The range is what `describe_parameters` advertises, so a tool
that reads it expects a set outside it to be refused. A refused set's reason
string names the range. ADOPT-BOUNDED: a request rclc accepts can be refused,
never the reverse.

**Local typed sets follow rclc exactly**: type check, callback, write. Read-only
and range are not checked. The owner of a read-only parameter can still change
it, which is what rclc's read-only means ("read-only to the outside"). This is
the one place the C path and the C++/Rust path differ, and the RFC-0036
divergence catalog records it.

### D8 — `/parameter_events`

When `notify_changed_over_dds` is set, the server creates one publisher on
`/parameter_events`: type `rcl_interfaces/msg/ParameterEvent`, QoS
KEEP_LAST(1000) RELIABLE VOLATILE (upstream's `parameter_events` profile). It
publishes on every successful add, set and delete, from both the local and the
service path, with one parameter in `new_parameters`, `changed_parameters` or
`deleted_parameters`, and `node` set to the node's fully qualified name. That is
what rclc emits, and it is what rqt_reconfigure, `rclcpp::ParameterEventHandler`
and `AsyncParametersClient::on_parameter_event` consume.

The event is serialised straight from the store into the publisher's buffer.
The `ParameterEvent` struct is never built. The publisher takes one publisher
slot and no queryable slot. The emitter lives in `nros-node` behind
`param-services`, so the C++ and Rust surfaces can turn it on later without a
second implementation.

### D9 — Retirements

Removed, with every caller migrated in the same phase:

* `nros_parameter_server_t`, `nros_parameter_server_init(server, storage,
  capacity)` and the rest of the caller-storage family, plus the deprecated
  `nros_param_*` aliases. The type name is reused by nothing, so old code fails
  to compile rather than binding to a new contract.
* `nros_executor_register_parameter_services` and every
  `nros_executor_*_param*` / `*_param*_on` function.
* C use of `nros_cpp_get_param_*`. A C component gets an
  `rclc_parameter_server_t` for its node from the component context
  (`rclc_parameter_server_init_default(&server, node)` works once the
  component's node is bound), and the shim becomes C++-internal.

The census hook (`on_param_declare`, issue 1556) moves to the new declare path.
`check-census-hooks-complete` and `check-c-executor-remove-coverage` follow it.

## Extensions

These are in `<rclc_parameter/rclc_parameter.h>` with rclc-style names because
rclc has no equivalent:

| Function | Why |
| --- | --- |
| `rclc_parameter_declare_{bool,int,double,string}(server, name, default)` | Declare with a default **and keep a launch-file value** for that name, as rclcpp does. rclc's `add_parameter` + `set_*` overwrites a launch value, which matches upstream rclc (it has no overrides) and is kept. Docs and examples lead with `declare_*`. |
| `rclc_parameter_{get,set}_string(server, name, buf, len)` | rclc is bool/int/double only. Strings are bounded by `MAX_STRING_VALUE_LEN`. |
| `rclc_parameter_{get,set}_{bool,int,double,string}_array` | Arrays are bounded by `MAX_ARRAY_LEN` (32). |
| `rclc_parameter_get_type`, `rclc_parameter_list` | Read-side helpers the executor family had. |

`RCLC_PARAMETER_STRING`, `_BYTE_ARRAY`, `_BOOL_ARRAY`, `_INTEGER_ARRAY`,
`_DOUBLE_ARRAY` and `_STRING_ARRAY` extend `rclc_parameter_type_t` with the
`rcl_interfaces` numbering (4..9). Values 0..3 already match rclc.

## The bounded envelope (what a porting user gets)

| Upstream behaviour | nano-ros | Disposition |
| --- | --- | --- |
| Server allocates its storage | No allocation; capacity is build-time | ADOPT-BOUNDED |
| Server init before executor | `RCL_RET_NOT_INIT` | ADOPT-BOUNDED |
| `Parameter` is a rosidl message | Member-path compatible, not rosidl ABI | ADOPT-BOUNDED (rosidl construction is REFUSE-LOUD) |
| Names and strings unbounded | `MAX_PARAM_NAME_LEN`, `MAX_STRING_VALUE_LEN` | ADOPT-BOUNDED, over-length returns `RCL_RET_INVALID_ARGUMENT` |
| Range never enforced | Enforced on the service path | ADOPT-BOUNDED |
| 5 executor handles (humble) | 6, rolling's count, including set_atomically | ADOPT (rolling) |
| Return codes | Same names; numeric values as `rcl_compat.h` already maps them; `RCLC_PARAMETER_*` keep upstream values | ADOPT |

## Interop proof

A live interop cell (`interop::CELLS`) runs a C node built only on this API and
checks, against a real ROS 2 peer:

1. `ros2 param list` / `get` / `describe` show its declarations, including a
   launch value that `declare_*` kept.
2. `ros2 param set` reaches the callback with `old` and `new`, and a `false`
   return is reported as rejected.
3. `ros2 param set` outside an advertised range is refused, and one on a
   read-only parameter is refused.
4. An rclcpp `ParameterEventHandler` receives a `ParameterEvent` for a local set
   and for a remote set.

## Alternatives rejected

* **`nros_*` canonical with `rclc_*` aliases.** That is the `rcl_compat.h`
  pattern, and it was the first proposal. Rejected by the maintainer on
  2026-10-11: the C API's purpose is ROS interop, and two names for one function
  is the drift RFC-0089 exists to stop.
* **Keep the executor family and add rclc wrappers over it.** That leaves two
  public families, which is the problem.
* **Bind late (server init before the node is bound, declarations refused until
  `add_parameter_server`).** Rejected 2026-10-11: it allows an object that exists
  and refuses every call, which is a worse error than refusing init.

## Open questions

* Whether the whole C API moves to `rcl_*`/`rclc_*` primary names is out of
  scope here. This RFC decides the parameter surface only.
