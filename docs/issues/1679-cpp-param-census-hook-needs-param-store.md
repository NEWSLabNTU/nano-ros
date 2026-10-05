---
id: 1679
title: "The C++ parameter census hook sits inside the `param-store` cfg, so a census of an image without `param_services` records no parameters"
status: open
type: bug
area: [api, cli]
severity: low
found: 2026-10-05
related: [1649, 1556, phase-463]
---

## What

Every `nros_cpp_node_declare_param_*` entry point in
`packages/api/nros-cpp/src/params_shim.rs` calls
`nros::census_hooks::on_param_declare(...)` INSIDE

```rust
#[cfg(all(feature = "param-store", feature = "rmw-cffi"))]
{ ... nros::census_hooks::on_param_declare(name, &pv); declare_on_node(...) }
```

The comment beside it says the hook is "unconditional call, `#[cfg]` body" and
"sits BEFORE the store so a declaration the code makes is recorded whatever the
store answers". The first half is true of the hook's own body. The CALL SITE is
not unconditional: it is compiled only when the store is. A C++ image whose
bringup does not declare `param_services` (or otherwise turns on
`param-store`) declares its parameters into the `#[cfg(not(...))]` arm, and the
census never sees them.

## How it was found

Measuring issue 1649's parameter rows on `examples/workspaces/cpp`
(2026-10-04): a temporary `declare_parameter<...>` in `Talker.cpp` plus a
contract `params:` row. `nros ws entity-census take` recorded no parameters,
so the census and the contract disagreed (`param-phantom`), until
`features = ["param_services"]` was added to the image's `[system]`.

## Why it matters

The census is the program's own statement of what it declares (issue 1556).
A census that depends on a build feature unrelated to WHAT the code declares
reports a different program depending on the feature set, and a disagreement
with the contract is then about the build, not the code.

## Fix direction

Move the hook call out of the `param-store` cfg (it is already a no-op unless
`metadata-mode` is on), in every declare entry point — the bool/int/double
ones and the two that build a `pv` — and add a census test over an image that
declares a parameter WITHOUT `param_services`.
