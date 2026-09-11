# Message Binding Generation

**Moved.** The message-generation reference now lives in the book:
[Message Binding Generation](../../book/src/user-guide/message-generation.md).

This file is kept only so that older links do not 404 — `README.md`, `CLAUDE.md`
and the `nros` crate's rustdoc all point here. The book page carries everything
this one did (the `package.xml` schema, the workflow, the generated output
structure, the bundled interfaces, the CMake side) plus two sections this one
never had: why message crates are RMW-agnostic, and the one table that says
which CMake spelling to use.

Two reasons not to read the old text, rather than one:

- It named `packages/codegen/packages/nros-cli/` as the generator's home. That
  directory has not existed since the codegen submodule was folded in; the CLI
  is `packages/cli/`, built by `just setup-cli` (contributor) or
  `./scripts/bootstrap.sh`.
- It sold `nros generate-rust --config / --nano-ros-path / --nano-ros-git` as
  the way to get a leaf's `[patch.crates-io]` table. The user command is
  **`nros sync`**. The flags still parse (`--config` is an accepted alias of
  `--generate-config`), and `--nano-ros-path` is documented in the binary's own
  help as a no-op kept for back-compat — but what a leaf needs is no longer a
  hand-maintained `.cargo/config.toml`. Under
  [RFC-0098](../design/0098-generated-leaf-build-config.md) `nros sync` writes
  the patch rows, the board's cargo settings and the derived `[env]` into
  `build/<image>/nros-cargo.toml`, and cargo reads that file through `--config`.

## The short version

```bash
cd <leaf>
nros sync            # generated/ msg crates + build/<image>/nros-cargo.toml
nros build           # or: nros build <image-id>
```

## See Also

- [Message Binding Generation](../../book/src/user-guide/message-generation.md) — the live page
- [RFC-0023](../design/0023-codegen-workspace-discovery.md) — the design
- [RFC-0098](../design/0098-generated-leaf-build-config.md) — where a leaf's
  build settings come from
