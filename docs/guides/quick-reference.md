# Quick Reference

**Moved.** The manual-testing cheat sheet now lives in the book:
[Quick Reference](../../book/src/reference/build-commands.md).

This file is kept only so that older links do not 404. The book page is a strict
superset — the same Manual Testing / UDP / TLS / ROS 2 Interop / Actions / Docker
/ QEMU / Zephyr sections, plus a "Finding Commands" preamble for `just --list`
and its groups, and a section on test profiling.

The text that was here is not just duplicated, it is wrong in one way the book
page is not: it drove examples with `cargo run --features zenoh` and
`cargo run -p native-rs-talker`. `zenoh` has not been the feature name for some
time (it is `rmw-zenoh`), examples are standalone packages so there is no root
workspace to take `-p`, and the RMW is no longer a cargo feature a user picks —
it is `[system] rmw` in the leaf's `system.toml`
([RFC-0098](../design/0098-generated-leaf-build-config.md)). The flow is
`nros sync`, then `nros build` (or `cargo build --config
<leaf>/build/<image>/nros-cargo.toml` if you drive cargo yourself).

## See Also

- [Quick Reference](../../book/src/reference/build-commands.md) — the live page
- [zephyr-setup.md](zephyr-setup.md) — Zephyr workspace setup
