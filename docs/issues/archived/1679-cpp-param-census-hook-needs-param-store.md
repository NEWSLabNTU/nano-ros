---
id: 1679
title: "The C++ parameter census hook sits inside the `param-store` cfg, so a census of an image without `param_services` records no parameters"
status: resolved
type: bug
area: [api, cli]
severity: low
found: 2026-10-05
related: [1649, 1556, phase-463]
resolved_in: "branch issue-1679 (fix(#1679) PR)"
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

## Resolution (2026-10-05)

**Fix.** Each of the seven `nros_cpp_node_declare_param_*` in
`packages/api/nros-cpp/src/params_shim.rs` calls
`census_hooks::on_param_declare` as its FIRST statement, before the
`param-store` cfg split, through a small `census_name(node, name)` helper. The
helper returns `None` without `metadata-mode`, so on a firmware build the call
is dead code and reads neither the name nor the value. The value is built
from the arguments the same way the store arm builds it (`from_string` /
`from_*_array`; a value the store would refuse as FULL is not recorded,
which matches the old in-arm behaviour). `slice_or_empty` is ungated because
the array forms now name it in every build.

**Measured.** A new lib test,
`metadata_hooks::census_without_store_tests`, uses `std,rmw-cffi,metadata-mode`
with NO `param-store`, the feature set of a census of a C++ image whose bringup
declares no `param_services`. It declares one parameter through each of the
seven entry points, asserts each answers `UNSUPPORTED`, and asserts the census
records all seven. Against the old shim it fails with `"parameters":[]`, the
issue's report verbatim. With the fix it passes. `just check
census-hooks-complete` runs it in its own scoped target dir
(`target-check-census-hooks-no-store`, gitignored), because a different feature
set writes a different generated header (issue 1354). Clippy `-D warnings`
(1.99.0) is clean for `std,rmw-cffi,param-store`,
`std,rmw-cffi,param-store,metadata-mode`, `std,rmw-cffi,metadata-mode` and
`std,rmw-cffi,param-services,platform-posix`.

**Gate.** `check-census-hooks-complete` asked only whether the hook appears
somewhere in an entry point's body, and it reads the text of every cfg arm. A
call confined to one arm therefore passed, which is how this shipped. The gate
now also refuses a hook whose first call comes after a `#[cfg(` at the body's
own nesting level. Against the old shim it names all seven declare variants.
The new self-test mutation 2b moves one variant's hook back inside the store
arm and requires a red. All 26 entry points pass on the fixed tree.

**The C side does not have this shape.** `nros-c`'s
`nros_executor_declare_param_*` live entirely inside the `service_backed`
module (`#[cfg(all(feature = "param-services", feature = "rmw-cffi"))]`), so
a C image without `param_services` that declares a parameter fails to LINK,
which is loud. It never compiles a silent arm. Within that module the hook is
unconditional. The Rust `node_runtime` sink already calls the hook before its
`param-services` cfg ("whether or not `param-services` gives this image a
store at all").

**Issue 1680 is not closed by this.** The census is still feature-dependent:
`param_services` / `lifecycle` register their service families through the
RMW, which the recorder sees, and those rows count toward
`census_callback_slots`. See that issue's resolution.
