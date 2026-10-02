---
id: 1613
title: "nros-log's `log` bridge files every facade record under the DEFAULT logger,
  so a Zephyr Rust image's `rustapp:` lines now read `nros:`"
status: resolved
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

## Resolution

`LogCrateBridge::log` now files a record under the `log` target whenever the
target resolved to no interned logger (`bridged_logger_name` in
`packages/core/nros-log/src/log_compat.rs`); the default logger still lends its
THRESHOLD, so filtering is unchanged. An interned logger keeps its own name, and
an empty target keeps the resolved logger's. `CONFIG_RUST_ALLOC` is not involved.

Measured on a real native_sim `rust/talker` (zenoh, Zephyr 3.7, live router):

    before (1324):  <inf> nros: nros: Publishing: 'Hello World: 1'
    after:          <inf> nros: rustapp: Publishing: 'Hello World: 1'
                    <inf> nros: rustapp::app_main: Waiting for messages

The leading `nros:` is the Zephyr `LOG_MODULE` the platform sink writes through,
where zephyr-lang-rust's logger used `rust:`; the origin column is the record's
own `log` target, which is the module path when the call site names none.

Tests: `log_compat` unit cases for an un-interned target, an empty target and an
interned logger; `tests/log_bridge_keeps_the_target.rs` asserts DELIVERY through
a real `LogSink`, and with the old line restored it fails with
`[("nros", Info, ...), ("nros", Warn, ...)]`. `log-compat` is enabled by no
workspace member, so neither test (nor the pre-existing `log_compat` unit tests)
ran in `test-unit`; both now run in `just check required-features-tests`.

Not measured: an mps2-an385 Rust image's console (same crate, same bridge).
