# Phase 265 — `ws sync` patches → `.cargo/config.toml` (toml_edit); unify on independent Rust packages

Status: **Planned (2026-06-23)** · Resolves [issue 0094](../issues/0094-ws-sync-toml-line-scanner-fragility.md)
· RFC-0026 (workspace model), RFC-0040 (board crates) · Follows phase-220.E / phase-244 (the
current `ws sync` patch writer).

> **Goal.** Stop `nros ws sync` from ever editing a consumer `Cargo.toml`. It instead owns a
> `[patch.crates-io]` table in a per-package **`.cargo/config.toml`**, written with a
> format-preserving `toml_edit` DOM. And **unify the nros-workspace topology** on
> **independent Rust packages** (no root cargo `[workspace]`), so orchestrated nodes build
> per-platform without cargo-workspace feature-unification. This eliminates issue 0094's whole
> bug class (there is no manifest edit left to corrupt) and makes the multi-platform build the
> default-correct path.

## Why

### 0094 — the manifest editor is a line scanner
`ws sync` rewrites the consumer `Cargo.toml`'s `[patch.crates-io]` with a **line-based text
scanner** (`packages/cli/nros-cli-core/src/cmd/ws.rs`: `splice_patch_block` /
`extract_patch_table` / `strip_managed_block` / `extract_consumer_registry_nros_deps` + a
`BEGIN…END` managed-region marker). It is correct for the canonical shape it emits, but
TOML-equivalent shapes break it (0094 A–F: quoted `[patch."crates-io"]` → duplicate table;
explicit `[dependencies.nros]` → missing patch; doubled managed block; `"""…"""` false-match;
unescaped path; empty bare header). A/B/C were hardened in `68e167275`; D/E/F + the fragility
remain. The root cause is editing a hand-authored manifest with a non-parser.

### Two topologies exist today (the design constraint)
- **`examples/workspaces/rust/`** — cargo **workspace**: root `Cargo.toml [workspace]
  members=[…]` (native + freertos + nuttx + threadx + esp32 entries; zephyr `exclude`d),
  shared root `generated/`, one root `[patch.crates-io]`.
- **`examples/workspaces/mixed/`** — **independent packages**: NO root `Cargo.toml`; each Rust
  pkg is isolated (its own `[workspace]` table), **per-package `generated/`**, per-package
  `[patch.crates-io]`. (C/C++ pkgs aren't cargo at all.)
- **`examples/<plat>/<lang>/<example>/`** — standalone: per-package, isolated, copy-out.

### The cargo-workspace model fights multi-platform
A single root cargo `[workspace]` over per-platform entries has hard consequences (verified
against `rust/`):
1. **Bare `cargo build` is forbidden** — building all members unifies shared-dep features →
   `nros/std` (native) collides with `nros/alloc,panic-halt` (freertos). Must *always*
   `-p <entry> --target <T>`.
2. **Whole-workspace resolve every build** — `-p native_entry` still resolves every member's
   manifest (incl. esp32 xtensa-only deps) into the one lock; a member that can't resolve on
   the host can break unrelated builds. Adding a platform couples it to all others.
3. **One Cargo.lock** pins all platforms together.
4. **No per-package copy-out** — members carry no `[workspace]`; a node pkg can't be lifted out
   and built alone (breaks the standalone-copy-out promise).
5. **Embedded members need per-target config anyway** — `rust/.cargo/config.toml` already grew
   `[target.thumbv7m]`/`[riscv32]`/`[nuttx]` + build-std, and zephyr had to be excluded.

The independent model (`mixed/`) avoids all five: each package builds standalone for its own
platform (no unification, no cross-member resolve), copy-out works, entries path-dep node pkgs
and compile them for their target. Cost: per-package `.cargo/config.toml` + per-package
`generated/` (more files, duplicated bindings).

### `.cargo/config.toml [patch]` works for unpublished crates — proven
- `paths` override is **out** — cargo docs: "only works for crates published to crates.io"
  (nros crates aren't).
- `[patch.crates-io]` in `.cargo/config.toml` **is** stable (cargo 1.96; the repo already uses
  it for the NuttX `libc` build-std patch) and **resolves unpublished `nros-core = "*"` with a
  100% clean manifest** (empirical probe: config-patch present → resolve `rc=0`; removed →
  `no matching package nros-core`). It is auto-discovered up the directory tree, so plain
  `cargo build` works — **no root cargo `[workspace]` required** (proven: an ancestor
  `.cargo/config.toml` patch applies to a package built beneath it with no root Cargo.toml).
- `--config`/CLI patch is rejected for examples (wrapper-only; breaks plain `cargo build`).

## Decision

1. **`ws sync` never edits a consumer `Cargo.toml`.** It owns `[patch.crates-io]` in a
   **`.cargo/config.toml`**, one per Rust package (at the package dir). Consumer manifests keep
   clean `version = "*"` deps.
2. **Unify the nros-workspace topology on independent Rust packages.** No root cargo
   `[workspace]`. Every Rust package (node pkg + each per-platform entry) is an isolated cargo
   package (`[workspace]` table) with its own `.cargo/config.toml` + own `generated/`.
   `examples/workspaces/rust/` converges to the `mixed/` shape (drop the root `[workspace]`;
   per-package configs).
3. **All TOML editing goes through `toml_edit`** (format-preserving DOM) — both the write
   (`config.toml [patch.crates-io]`) and the read (consumer/generated `Cargo.toml` dep scan).
   No line scanners.

This is forced as much as chosen: per-package `generated/` means `pkgA/generated/std_msgs` and
`pkgB/generated/std_msgs` are the **same crate name, different paths** — one shared
`[patch.crates-io]` table can't express that, so per-package is the only correct scope.

## Design

### Write path (replaces render + splice + strip)
Per Rust package authority:
```rust
let cfg = authority_dir.join(".cargo/config.toml");
let mut doc: DocumentMut = read_or_empty(&cfg).parse()?;
let patch = doc["patch"]["crates-io"].or_insert(implicit_table());   // bare/quoted/dotted = same key
patch.retain(|_, v| !decor_has_marker(v));                           // evict prior nros-managed
for (name, rel) in managed_sorted {                                  // generated crates + nros-core/serdes + 220.E/244-E3 extras
    let mut it = InlineTable::new(); it.insert("path", rel.into());
    let mut item = value(it); set_suffix_decor(&mut item, "  # nros-managed");
    patch.insert(&name, item);
}
if patch.is_empty() { doc["patch"].as_table_mut().remove("crates-io"); }   // 0094 F
write_atomic(&cfg, &doc.to_string());                                       // temp + rename (unchanged)
```
- **Ownership via per-key `# nros-managed` decor marker** (not a region): the config table
  mixes sync-owned patches with user content (the `libc` patch, `[target]`/`[env]`); the marker
  is how sync knows what to evict/replace. Preserves user keys + their decor. Sorted managed
  keys → diff-stable. Evicts stale generated crates (a removed msg pkg) by the marker.
- **Atomic write** (temp + `rename`) preserved.

### Read path (managed-set discovery) — DOM, not scanners
`extract_consumer_registry_nros_deps` + `extract_cargo_path_deps` → walk `doc["dependencies"]`,
`["dev-dependencies"]`, `["build-dependencies"]`, `["target"][cfg][kind]` as DOM tables. Inline
`name = { version = … }` and explicit `[dependencies.name]` are the **same DOM shape** → 0094 B
vanishes. `package.xml` → codegen → generated-crate-names is unchanged (separate XML path; only
feeds the generated-crate set, category 1).

### Managed-set + path policy (unchanged)
Three key sources (`render_patch_block` today): (1) generated msg crates (`<pkg> =
{ path = "generated/<pkg>" }`); (2) hardcoded minimum `nros-core` + `nros-serdes`; (3)
registry-style `nros-*`/`cyclonedds-sys` from the consumer + generated `Cargo.toml`s, mapped via
the static `nros_crate_path_lookup` table. Paths via `pathdiff` from the config's package dir.

### Why all 6 0094 cases die by construction
A (quoted) + B (dotted) → same DOM key path. C (doubled region) → no region concept. D (`[patch]`
in `"""…"""`) → strings are nodes. E (path escaping) → serializer escapes. F (empty header) →
`remove` when empty.

### Migration (one-time)
- **Strip the `BEGIN…END` managed block from every consumer `Cargo.toml`** (workspace root +
  every standalone/isolated pkg). Managed entries re-materialize in the per-package
  `.cargo/config.toml`. The user's `version = "*"` deps stay.
- **`examples/workspaces/rust/` converges to independent**: drop the root `[workspace]`; give
  each member its own `[workspace]` + `.cargo/config.toml` + `generated/` (it already has
  per-pkg `generated/`). zephyr_entry already standalone.
- A pre-existing hand-authored manifest `[patch.crates-io]` for a *managed* crate would collide
  with the config patch ("duplicate patch") — migration moves managed entries out; document that
  managed crates are config-owned.

## Work items
- **W1 — config writer + toml_edit + marker + atomic write + tests.** New `write_patch_config`
  (DOM, per-key marker) replacing `render_patch_block`/`splice_patch_block`/`strip_managed_block`.
  Port the 18 patch_block tests to assert observable output (one patch table, user keys preserved
  incl. `libc`, managed evicted+reinserted, idempotent, stale-generated eviction). New tests:
  quoted/dotted equivalence, `"""`-immunity, empty-set removal, legacy-block migration.
- **W2 — read-side DOM.** `extract_consumer_registry_nros_deps` + `extract_cargo_path_deps` →
  `toml_edit`. Keep `package.xml` parsing as-is.
- **W3 — topology convergence + example migration.** Drop `rust/` root `[workspace]`; per-pkg
  `[workspace]` + `.cargo/config.toml` + `generated/`. Strip `BEGIN…END` from all consumer
  `Cargo.toml`s. Re-run `ws sync`; rebuild native + a per-platform lane to confirm resolve.
- **W4 — delete dead scanners.** Remove `splice_patch_block`, `extract_patch_table`,
  `strip_managed_block`, `is_patch_crates_io_header`, `strip_toml_key_quotes`, the `BEGIN/END`
  constants, and the `RenderedBlock` shape. Update `run_clean` / `run_doctor` to read config.

## Sequencing
W1 (writer, behind the existing call site) → W2 (read DOM) → W3 (flip examples + migrate) →
W4 (delete). Each ships green `just check` + the cli-core unit tests; W3 also rebuilds native +
one embedded lane.

## Acceptance
- `ws sync` writes `[patch.crates-io]` only into `.cargo/config.toml`; **no consumer `Cargo.toml`
  is modified** by sync (grep-clean after a sync run).
- All nros-workspace + standalone examples build (native + ≥1 embedded lane) with plain
  `cargo build` / the existing fixture lanes; node-pkg copy-out builds standalone.
- The 0094 A–F cases are structurally impossible; issue 0094 resolved.
- No root cargo `[workspace]` in any nros workspace example; `rust/` == `mixed/` topology.

## Notes / risks
- `toml_edit 0.22` is already a `nros-cli-core` dep (no new dep).
- Blast radius: the canonical writer for every consumer `Cargo.toml` + a one-time topology flip
  of `rust/` + a churn pass across all examples (patches move manifest→config). It's the fix for
  the recurring churn, so the one-time diff is acceptable.
- Cosmetic: cargo warns on a patch "not used in the crate graph"; managed entries are used (no
  warn) — unlike the intentional `libc` one.
- Keep the static `nros_crate_path_lookup` table + the package.xml→codegen path as the SSoT for
  the managed set; only the read/write *mechanism* changes.
