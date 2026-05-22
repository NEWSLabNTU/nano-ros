# Phase 167 — Absorbed by Phase 118

**Status.** Closed by Phase 118.E.

The NuttX Rust collapsed-shape link regression was tracked as
**118.E.4**. The fix moved the Rust crates to
`examples/qemu-arm-nuttx/rust/<case>/` and adjusted local dependency /
`[patch.crates-io]` paths for the depth-4 layout.

## Original Scope

Restore `cargo build` on collapsed-shape
`examples/qemu-arm-nuttx/rust/<case>/` directories. The depth-4 collapsed
layout failed with:

```text
undefined reference to __libc_init_array
undefined reference to __libc_fini_array
```

while the legacy depth-5 layout
`examples/qemu-arm-nuttx/rust/zenoh/<case>/` linked with the same
toolchain/config.

## Resolution

The link failure was caused by stale depth-5 relative paths after the
layout collapse. The depth-4 crates now point at `../../../../packages/*`
and `../../../../third-party/nuttx/libc`; fixture builders and just
recipes use the collapsed paths.
