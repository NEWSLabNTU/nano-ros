---
id: 1613
title: "nros-log's `log` bridge files every facade record under the DEFAULT logger,
  so a Zephyr Rust image's `rustapp:` lines now read `nros:`"
status: open
type: bug
area: [logging, zephyr]
severity: low
found: 2026-10-01
related: [1324, 1123]
---

## What

Issue 1324 replaced `zephyr::set_logger()` in both Zephyr entry macros with
`nros_platform::log::install_log_crate_bridge()` (nros-log
`log_compat::LogCrateBridge`), because the former's full-`CONFIG_LOG` arm needs
`CONFIG_RUST_ALLOC`, which 1324 refuses. Measured on native_sim
`rust/talker`:

    before: <inf> rust: rustapp: Publishing: 'Hello World: 1'
    after:  <inf> nros: nros: Publishing: 'Hello World: 1'

`LogCrateBridge::log` resolves `nros_log::get_logger(record.target())`, which
returns `DEFAULT_LOGGER` ("nros") for any target nobody interned, and the
record then carries that logger's name. The message survives (tests grep the
message through `nros_tests::output`), the origin does not.

## What would fix it

Carry the `log` target as the record's `logger_name` when no logger is interned
for it (it is a `&str` that outlives the dispatch), keeping the default logger's
THRESHOLD for filtering. A unit test in `log_compat` on an un-interned target.
