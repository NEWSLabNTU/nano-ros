---
id: 250
title: "rosidl-codegen silently flattens multi-dimensional IDL arrays to the first dimension"
status: open
type: bug
severity: low
area: codegen
---

## Finding (release-prep audit 2026-07-24)

`packages/cli/rosidl-codegen/src/idl_generator.rs` (~line 312):

```rust
IdlType::Array(inner, dimensions) => {
    // Arrays in IDL can be multi-dimensional, but we'll handle the first dimension
    // TODO: Support multi-dimensional arrays properly
    let size = dimensions.first().copied().unwrap_or(1);
```

A multi-dimensional IDL array generates a type sized by its FIRST dimension
only — silent wrong layout (wire-incompatible with the ROS 2 peer, no error,
no warning). Standard ROS 2 `.msg` cannot express multi-dim arrays, so the
exposure is `.idl`-sourced interface packages only — rare, but the failure
mode is silent data corruption at the CDR boundary, not a build error.

## Fix

Reject multi-dim arrays loudly at generation time ("multi-dimensional IDL
arrays not supported") until proper support lands. Silent flattening is the
worst of the three options.
