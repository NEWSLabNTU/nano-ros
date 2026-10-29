---
id: 2123
title: "A C/C++ multi-session spec carries no session properties, so its domains cannot have an RMW file"
status: resolved
type: limitation
area: [api, c-api, cpp, rmw, orchestration]
severity: medium
found: 2026-10-29
resolved: 2026-10-29
related: [issue-0436, issue-2062]
---

## What

RFC-0109 D6 gives each `[[domain]]` its own RMW file (`[[domain]] config`),
lowered to that session's backend properties (D8). The Rust road carries them:
`nros_node::executor::SessionSpec` has

```rust
pub properties: &'cfg [(&'cfg str, &'cfg str)],
```

and the bridge sessions that `nros::main!` opens fill it per domain (phase-514
W3b, and the per-image `nros-bridge.<image>.toml` of issue 2062).

The C and C++ mirrors do not. `NrosCppSessionSpec`
(`packages/api/nros-cpp/src/lib.rs`, `include/rclcpp/detail/nros_cpp_ffi.h`)
and `rclc_session_spec_t` (`packages/api/nros-c/include/rclc/bridge.h`) hold
`rmw`, `locator`, `domain_id`, `node_name` and `namespace_` only, and they are
kept field-for-field equal so a caller can use either entry point (issue
0436). `nros_cpp_init_multi` / `rclc_init_multi` therefore build every
`SessionSpec` with empty `properties`.

## Consequence

A C or C++ image that opens more than one session has no way to give a
non-default session its domain's RMW file. Only the default session gets one,
through the baked table. No field exists that could carry a domain's lowered
table across the C ABI. Phase-514 W3b states this gap and leaves it open.

## Fix shape

- Append `properties` + `properties_len` (an array of `{key, value}` C-string
  pairs) to both structs together, keeping them identical. Both are hand
  mirrors, so `check-ffi-struct-mirrors` must cover the new tail.
- Have the C/C++ codegen for a multi-session entry emit each domain's
  lowered table, from the same resolver the Rust road uses.
- Acceptance: a C++ bridge leaf naming two domain files opens each session
  with its own properties. Prove it the way the Cyclone property was proved
  in W3b (mutation-checked).

## Resolution

**ABI.** All three mirrors gain `properties` + `properties_len`, appended so
an existing aggregate initializer still compiles with NULL/0 = none:
- `NrosCppSessionSpec` (`nros_cpp_session_property_t` rows, cbindgen header
  regenerated);
- `rclc_session_spec_t`, with a new `rclc_session_property_t` of the same
  layout;
- the bridge's Rust `NrosSessionSpec`, whose rows are
  `nros_node::SessionProperty`.

The rows are borrowed for the open only, because a session open hands them to
its backend and keeps none. `rclcpp::SessionSpec` gains `with_properties`. The
PX4 bridge module spells the two new fields.

**One reader.** `nros_node::lent_session_properties` validates a lent table
and refuses rather than truncates. A NULL table with rows, or a row that does
not read, is INVALID_ARGUMENT; more rows than a session carries is FULL.
- `nros_cpp_init_multi` reads every spec's rows into one pool of
  2 x `MAX_SESSION_PROPERTIES`, with no allocator and no per-spec table on the
  caller's stack.
- `rclc_init_multi` reads them per spec.

**The table, at build time.** No codegen emits a C/C++ multi-session entry;
the in-tree callers are hand-written, so the second bullet of the fix shape
above became a tool rather than a template.
- `nros ws session-properties <bringup> --domain <name> --symbol <ident>
  --out <header> [--image <id>] [--row-type …]` writes `static const <row>
  <ident>[N]` plus `<ident>_LEN`.
- It uses the same resolution as the Rust road's `nros-bridge.toml`:
  `SystemToml::domain_file` (an image override first), then
  `RmwFile::session_properties`, so a Cyclone XML file travels whole under
  `config`.
- CMake: `nros_session_properties_header(BRINGUP … DOMAIN … SYMBOL … OUT …)`.
  It adds re-configure edges on the tool, the `system.toml` and the file.
- The strings go through one C escaper,
  `nros_orchestration_ir::session_properties::c_string_body`. It escapes
  control characters too, so a multi-line document stays one literal. The
  entry templates' `c_str` filter now uses it; before, it escaped only `\`
  and `"`.

**Gates and tests.**
- `check-ffi-struct-mirrors` gains a fourth family covering the three spec
  mirrors. Mutation-checked: a field dropped from the Rust mirror, and one
  dropped from the C header, are each caught.
- Runtime probe `init_multi_session_properties_runtime.cpp`, in `just check
  cpp`: three specs with three different tables (one empty), against the
  shared stub backend, which now records each session's properties as
  copies. It also checks the NULL-rows and oversized refusals. Negative
  controls, measured: rows dropped → 5 failures; every spec handed the first
  spec's rows → 3 failures.
- Unit tests: `lent_session_properties` (nros-node), `c_table` (escaping and
  refusals), and `domain_session_header` (zenoh lowered, Cyclone whole, an
  image override wins, an unknown domain and a locator-only domain refused).
- Smoke-built by hand: the CMake function against
  `examples/workspaces/bridge-cyclonedds`. A C file using both domains' headers
  in `NrosCppSessionSpec` compiles, and the zenoh rows match that bringup's
  `nros-bridge.toml`.

- Runtime (added after the fix merged): `rclc_init_multi_session_properties_runtime.c`
  in `just check cpp` runs the C reader. `rclc_init_multi` lives in
  `nros-bridge` behind `cffi` and reaches a C binary only through `nros-cpp`'s
  `bridge` feature, so the lane builds that archive shape in its own target
  dir. Same three cases as the C++ probe. Negative controls, measured: the
  spec's rows dropped -> 5 failures; every spec handed the first spec's rows
  -> 3 failures.
