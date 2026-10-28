---
id: 2123
title: "A C/C++ multi-session spec carries no session properties, so its domains cannot have an RMW file"
status: open
type: limitation
area: [api, c-api, cpp, rmw, orchestration]
severity: medium
found: 2026-10-29
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
