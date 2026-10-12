# Creating Examples

**Moved.** The contributor guide to adding an example now lives in the book:
[Creating Examples](../../book/src/internals/creating-examples.md).

This file is kept only so that older links do not 404. Everything it used to
carry — the canonical `examples/<plat>/<lang>/<example>/` layout, the Rust and
CMake consumption shapes, the per-platform notes and the add-an-example
checklist — is on the book page, and only the book page is maintained.

The text that was here described a shape the tree no longer has. It taught a
hand-written leaf `.cargo/config.toml` (`[build] target`, a QEMU `runner`,
`rustflags`, `[patch.crates-io]` rows) and the
`nros generate-rust --config --nano-ros-path …` invocation that wrote half of
it. Under [RFC-0098](../design/0098-generated-leaf-build-config.md) an example
states its board in `system.toml`, and `nros sync` generates every build setting
that choice implies into `build/<image>/nros-cargo.toml`. No build setting is
hand-written and nothing generated sits beside the package.

What an example leaf contains today: `system.toml` (board, RMW, domain,
`[[component]]` rows, network identity), a `Cargo.toml` naming NO board crate
— only `<pkg>_nros_selection = { path = "build/nros/selection" }`, the package
`nros sync` / `nros build` generate with the image's board crate, RMW backend
and runtime crates (RFC-0098 D6) — `package.xml`, `src/`, and **no
`.cargo/`**. So changing `[image.*] board` is the whole board switch; a board
crate named beside the selection package is refused at `nros sync`
([issue 1305](../issues/archived/1305-single-package-board-crate-dep-not-generated.md)).

## See Also

- [Creating Examples](../../book/src/internals/creating-examples.md) — the live page
- [Message Generation](../../book/src/user-guide/message-generation.md)
- [zephyr-setup.md](zephyr-setup.md) — Zephyr workspace setup
- [RFC-0098](../design/0098-generated-leaf-build-config.md) — why the leaf build
  configuration is generated from one board choice
