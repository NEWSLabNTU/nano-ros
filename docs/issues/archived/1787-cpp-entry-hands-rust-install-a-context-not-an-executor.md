---
id: 1787
title: "A C++ entry hands a Rust node's install its nros-cpp CONTEXT, and the Rust side casts it to an Executor, which is correct only while rustc puts the executor first"
status: resolved
type: bug
area: [c-api, cpp-api, codegen, memory]
severity: high
found: 2026-10-10
resolved_in: 2026-10-10
related: [1535, 0436, 1635, phase-257, phase-477]
---

# Two different objects travel as "the executor handle"

The Rust component-install seam, `__nros_component_<pkg>_install(node, executor,
self)` (`nros::node_runtime::install_node_typed_with_launch[_in]`), is
documented as taking a bare `*mut Executor<'static>`, and it casts what it gets
to one:

```rust
let exec: &mut Executor<'static> = unsafe { &mut *(executor as *mut Executor<'static>) };
```

A Rust entry passes exactly that (`RuntimeCtx::executor_handle()`). The
GENERATED C++ entry did not. For a Rust node it emitted:

```cpp
void* __exec = ::rclcpp::global_handle();
int32_t crc = __nros_component_<pkg>_install(nullptr, __exec, nullptr);
```

The tiered path emitted the same with the tier's own `executor`. Both are an
nros-cpp `CppContext*`: `{ tag: u64, executor: Executor, domain_id, … }`. Issue
0436 added the `tag` to tell exactly these two handle kinds apart, but only
nros-cpp's own entry points check it. The Rust seam cast straight through.

`CppContext` was `repr(Rust)`, so the executor's offset was whatever rustc
chose. A comment beside the struct claimed it stays at offset 0. Nothing
enforced that, and the tag-is-first comment and the executor-is-first comment
cannot both be true.

# Measured

- **On `main` it works by luck.** The handle the heartbeat's install receives
  in the mixed Zephyr image is `rclcpp::Node::GlobalStorageHolder<0>::storage`,
  and its first word is NOT the tag: rustc placed the executor at offset 0. The
  same was true at `85ae1d156`.
- **Change the layout and the image SEGVs.** Making `CppContext` `repr(C)`
  (tag at 0, executor at 8) and changing nothing else crashes the mixed image
  during boot, inside the heartbeat's registration:
  `Executor::lookup_node_sched` ← `NodeBuilder::build` ← `create_node` ←
  `rust_heartbeat_pkg::register` ← `install_node_typed_with_launch_in`. It is
  silent memory corruption that a field added to `CppContext` can arm. Issue
  1635 added such a field (`diag`) five days before this was found.

Only one image runs this path: `examples/workspaces/mixed` (a C++ entry with a
Rust node). That is also the image issue 1535 was about. That crash had a
different cause (issue 1566's short C buffer) and was already fixed.

# Fix

- **nros-cpp:** `CppContext` is `#[repr(C)]` and asserts
  `offset_of!(CppContext, tag) == 0`. The tag is therefore readable from a
  `void*` by anyone, and the executor is never at offset 0, so a missed unwrap
  fails every time instead of only on some layouts. The new export
  `nros_cpp_executor_inner(handle)` returns the executor inside a live context
  (NULL for anything else).
- **Codegen:** the C++ pack's Rust arm (`node_body.jinja`, both the single and
  the tiered setup) passes `::nros_cpp_executor_inner(<handle>)` and returns
  `NotInitialized` on NULL.
- **nros:** a new module, `nros::executor_handle`, is the ONE check for a
  `void*` executor handle. `CPP_CONTEXT_TAG` is defined there, and nros-cpp now
  reuses it instead of a second copy. `executor_from_handle` refuses null and
  REFUSES a still-tagged context, logging the seam's name and the remedy. The
  three seams that take a handle use it: `install_node_typed_with_launch`,
  `install_node_typed_with_launch_in` (the refusal is `-4`, checked before the
  class's slot storage is taken) and `install_contract_monitors`.

# Guards and negative controls

- `nros-cli-core` `codegen::entry::emit::tests_cpp::
  rust_node_install_receives_the_unwrapped_executor` and
  `rust_node_install_on_a_tier_receives_the_unwrapped_executor` assert the
  install's ARGUMENT on both paths. Against the pre-fix `node_body.jinja` both
  FAIL (measured, 0 passed / 2 failed). The `cpp_native_shapes` golden moves
  with them.
- `nros` `executor_handle::tests` (in the `env,std` lane, where the crate's
  lib tests link): a tagged buffer is refused, the word after it is accepted,
  null is refused.
- Runtime negative control: the fixed runtime with the OLD entry template
  boots and logs, instead of SEGVing:
  `install_node_typed_in: handed an nros-cpp executor CONTEXT where an Executor
  was expected; a C/C++ caller must pass nros_cpp_executor_inner(handle) (issue
  1535)`.
- With the fix, the heartbeat install receives `storage + 8` (the word before
  it is `0x6e524f5343505001`, the tag) and returns 0. The image publishes and
  receives.

# Sweep

```sh
# void* -> Executor casts in the API crates: the three handle seams now go
# through executor_from_handle; what remains is internal (spin.rs self-pointer,
# tick_one_cell, tests)
git grep -n "as \*mut Executor<'static>" -- 'packages/api/**/*.rs' 'packages/core/**/*.rs'
# every emitted Rust install: the C++ pack is the only emitter
git grep -n "_install(nullptr" -- packages/cli
```
