# Retired Configuration Files

<!-- type: how-to · level: 2 -->

These files used to carry configuration and no longer do. If a guide, an old
project or a blog post tells you to write one of them, its content now
belongs in `system.toml` (an image's settings, under `[image.<id>]`) or, for a
hand-written `no_std` app with no codegen, in the standalone `config.toml`
that [Configuration](../build/configuration.md) describes.

- **`nros.toml` — retired in full.** A workspace-root `nros.toml`
  is rejected by the CLI (`NrosTomlNotSupported`; migrate with the
  `nros-v0.5.0` tag's `nros migrate workspace` — the one-shot verb is retired
  on newer trees); the legacy per-package overlay is a deprecated fallback that
  `nros check` flags; the embedded-runtime role never shipped (no example
  ever declared one). If a doc tells you to write `nros.toml`, it predates
  the migration — the content belongs in `system.toml`.
- **Old `config.toml` schemas** — `[network]`/`[zenoh]`/`[scheduling]`, and
  the `[[transport]]` table — retired; the direct-mode
  `config.toml` above is the kept, supported shape.
