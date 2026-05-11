# Phase 111.B.1 — crates.io metadata audit

Goal of work item 111.B.1: every workspace crate in `packages/core/`,
`packages/zpico/`, `packages/xrce/`, `packages/dds/`, and
`packages/codegen/` has the metadata fields crates.io requires before
the first `cargo publish` (`authors`, `license`, `description`,
`repository`, `homepage`, `documentation`, `readme`, `keywords`,
`categories`), plus a `README.md` rendered on the package page.

Performed against `nano-ros-sentinel` HEAD (`84063c5b` + this commit)
on 2026-05-11.

## Scope

38 workspace member crates audited:

| Tree                       | Count | Notes |
|----------------------------|------:|-------|
| `packages/core/`           |   19  | nros + nros-{core,serdes,macros,params,rmw,rmw-cffi,node,c,cpp,sizes-build,platform,platform-{api,cffi,posix,nuttx,freertos,threadx,zephyr}} |
| `packages/zpico/`          |    6  | zpico-{sys,alloc,platform-shim,platform-custom,serial} + nros-rmw-zenoh |
| `packages/xrce/`           |    4  | xrce-{sys,platform-shim} + nros-rmw-xrce{,-cffi} |
| `packages/dds/`            |    1  | nros-rmw-dds (dust-dds is a submodule fork — out of scope, has its own workspace + license) |
| `packages/codegen/`        |    8  | nros-cli{,-core}, cargo-nano-ros, colcon-cargo-ros2 (crate name `colcon-nano-ros`), nros-codegen-c, rosidl-{bindgen,codegen,parser} (codegen is a submodule of `NEWSLabNTU/colcon-nano-ros`) |

## Workspace metadata changes

### Main workspace (`/Cargo.toml`)

Before:

```toml
[workspace.package]
version = "0.1.0"
edition = "2024"
license = "MIT OR Apache-2.0"
repository = "https://github.com/NEWSLabNTU/nano-ros"
rust-version = "1.75"
```

After:

```toml
[workspace.package]
version = "0.1.0"
edition = "2024"
authors = ["Lin Hsiang-Jui <jerry73204@gmail.com>", "NEWSLabNTU"]
license = "MIT OR Apache-2.0"
repository = "https://github.com/NEWSLabNTU/nano-ros"
homepage = "https://github.com/NEWSLabNTU/nano-ros"
documentation = "https://docs.rs/nros"
readme = "README.md"
rust-version = "1.75"
categories = ["embedded", "no-std", "network-programming"]
keywords = ["ros2", "robotics", "embedded", "no-std", "rtos"]
```

### Codegen submodule (`packages/codegen/packages/Cargo.toml`)

Repository URL was stale (`azewiusz/colcon-nano-ros`). Updated to
`NEWSLabNTU/colcon-nano-ros`. Added `homepage`, `documentation`, and
`rust-version`.

## Per-crate metadata changes

For 32 publishable crates (out of 38 — see "Not published" below),
added the following fields where missing:

- `authors.workspace = true`
- `homepage.workspace = true`
- `documentation.workspace = true`
- `readme = "README.md"`
- `keywords = [...]` (5-keyword tagline per crate, picked from
  domain-relevant terms — `ros2`, `embedded`, `no-std`, plus 2 crate-
  specific keywords)
- `categories = [...]` (1–4 crates.io categories, drawn from the
  upstream taxonomy)

The three special-cased crates (`nros`, `nros-c`, `nros-cpp`) already
had keyword + category sets that were better-tuned to their public
surface; left untouched. Only added the four missing fields
(`authors`, `homepage`, `documentation`, `readme`).

## Not published (`publish = false`)

| Crate                  | Reason |
|------------------------|--------|
| `nros-sizes-build`     | Build-script helper, only consumed in-workspace. |
| `nros-rmw-xrce-cffi`   | Depends on the C-side XRCE backend that isn't a Rust crate. |
| `nros-rmw-dds`         | Depends on unpublished `dust-dds` fork (jerry73204/dust-dds). |

These keep their existing metadata (description / license /
repository) for completeness but are gated out of `cargo publish`.

## README.md additions

Wrote 25 new README.md files (~12-line stub each):

**Main workspace (19):** nros, nros-c, nros-cpp, nros-core, nros-macros,
nros-node, nros-params, nros-platform, nros-platform-cffi, nros-rmw,
nros-rmw-cffi, nros-serdes, zpico-sys, zpico-alloc, zpico-platform-shim,
zpico-platform-custom, zpico-serial, xrce-sys, xrce-platform-shim.

**Codegen submodule (6):** cargo-nano-ros, nros-codegen-c, rosidl-codegen,
rosidl-parser, nros-cli, nros-cli-core.

Already had READMEs (untouched): nros-platform-api, nros-platform-posix,
nros-platform-nuttx, nros-platform-freertos, nros-platform-threadx,
nros-platform-zephyr, nros-rmw-zenoh, nros-rmw-xrce, nros-rmw-dds,
colcon-cargo-ros2, rosidl-bindgen.

Every README follows the same skeleton: title + one-paragraph blurb
linking back to the parent project, then a "License" footer pointing
at Apache-2.0 / MIT and the nano-ros project.

## Remaining caveats

1. **License inconsistency.** `[workspace.package]` declares
   `MIT OR Apache-2.0`, but several crates (`nros`, `nros-c`,
   `nros-cpp`, `nros-sizes-build`, `zpico-alloc`) hard-code
   `license = "Apache-2.0"`. Either pick one (current state preserves
   each crate's recorded intent) or sweep them all to the workspace
   default. **Decision deferred to 111.B.7** when `CONTRIBUTING.md`
   documents the contribution-license policy.

2. **Authors are sparse.** Main workspace uses a 2-entry list
   (`Lin Hsiang-Jui <jerry73204@gmail.com>`, `NEWSLabNTU`). Codegen
   uses a single entry. If individual contributors want crates.io
   attribution, expand this before the first `0.1.0` publish.

3. **`documentation = "https://docs.rs/nros"`** is set workspace-wide.
   Each crate inherits this — `docs.rs` will redirect `docs.rs/<crate>`
   to its own page regardless, so the field is correct as a
   "documentation hub" pointer but does not perfectly fan out per
   crate. Acceptable for the initial publish; revisit only if a crate
   needs a non-`docs.rs` documentation URL.

4. **Codegen `repository` was stale** (`azewiusz/...`) — fixed in this
   audit. Worth double-checking the GitHub Pages / docs.rs links in
   `book/` for any other lingering references to the old URL before
   tag-time.

5. **Some keyword choices are speculative.** The taxonomy targets
   crates.io's known-good keyword space (`ros2`, `embedded`, `no-std`,
   `robotics`, `rtos`). If `cargo publish` rejects any keyword for
   being unrecognised, swap on first publish failure — keyword
   constraints are documented at
   <https://doc.rust-lang.org/cargo/reference/manifest.html#the-keywords-field>.

## Verification

```bash
# Workspace parses cleanly post-edit:
cargo metadata --no-deps --format-version 1 | jq '.packages | length'
# → 38
```

A dry-run `cargo publish --dry-run -p <crate>` on a leaf crate
(e.g. `nros-core`) was deferred — it requires network + first-time
authentication. That validation lives in work item **111.B.4**
(`nros release publish --dry-run`).

## Files touched

- `Cargo.toml` (main workspace)
- `packages/codegen/packages/Cargo.toml` (codegen workspace, submodule)
- 27 × `packages/**/Cargo.toml`
- 25 × `packages/**/README.md`
- `docs/research/phase-111-B1-crates-io-metadata-audit.md` (this file)

## Next steps

- **111.B.2** Reserve crate names on crates.io with `0.0.0` placeholder
  publishes (per phase-doc risk note about name squatting).
- **111.B.3** Implement `nros release detect` — produces the topo-
  sorted publish plan + validates that no crate is missing
  crates.io-mandatory fields.
- **111.B.4** Implement `nros release publish --dry-run` — first end-to-
  end "would the publish work" rehearsal. Likely surfaces a handful of
  field-format issues that this audit doesn't catch (e.g. README path
  case-sensitivity, license-file vs license-expression, long
  description rejection).
