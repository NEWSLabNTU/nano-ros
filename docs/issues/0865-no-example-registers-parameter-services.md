---
id: 865
title: "parameter services are implemented and tested but undiscoverable: no
  example calls them, and the C header declares the entry point unconditionally
  so a caller without the feature gets a bare `undefined reference`"
status: open
type: bug
area: examples, docs
related: [issue-0864]
---

## What is actually true

An earlier revision of this issue claimed the parameter-service capability was
"implemented and never demonstrated". That was filed on incomplete evidence and
is **wrong**. The capability is exercised and tested:

```
packages/testing/nros-tests/bins/param-chatter-talker/src/main.rs:30
    .register_parameter_services()
packages/testing/nros-tests/tests/params.rs:177
    fn test_ros2_param_list(zenohd_unique: ZenohRouter)
```

That test drives a real `ros2 param list` against a running node. The path works
and is covered.

Nor is "no example calls it" an oversight. Phase-277 W3.a moved the
`param-services` build **out** of the talker example and into a dedicated
fixture bin precisely so the example would stay cfg-free, and recorded that
intent in `examples/fixtures.toml`. Adding the call back to an example would
reverse a considered decision.

## The real defect: it fails unhelpfully, and late

Two narrower things are wrong, and they share a shape.

**1. The C header declares the entry point unconditionally.**
`nros_generated.h:3213` declares:

```c
NROS_PUBLIC nros_ret_t nros_executor_register_parameter_services(struct nros_executor_t *executor);
```

with no `#if` guard and no note. The implementation is gated:

```rust
#[cfg(all(feature = "param-services", feature = "rmw-cffi"))]   // parameter.rs:769
```

`param-services` is not a default feature. So a C caller who reads the header,
writes the obvious call, and builds gets:

```
main.c:(.text+0x302): undefined reference to `nros_executor_register_parameter_services'
```

A linker error naming a symbol is the least informative way to say "you need to
enable a capability". The header should either be guarded by the same condition
or carry the requirement in its doc comment — the declaration currently promises
something the library may not contain.

**2. The absence is indistinguishable from a fault.**
A node built without the feature answers `ros2 param list` with silence, which
looks exactly like a node whose parameter interface is broken. This is the same
observer problem as hidden `_action/` services: the system is behaving correctly
and nothing says so.

## Fix direction

- Guard the declaration, or document the required capability at the declaration.
  A caller should learn the requirement from the header, not from `ld`.
- Say in the parameter docs which capability is needed and how a CMake consumer
  asks for it (`nros_feature_set` capability `param_services`).
- Consider one line at node startup when parameter services are absent, so an
  empty `ros2 param list` explains itself.

## Not to do

Do not add the call to an example to "demonstrate" it. That is what phase-277
W3.a deliberately undid, and it would make the example's build feature-dependent
again.
