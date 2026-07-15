# Phase 265 — `ws sync` writes `[patch.crates-io]` to `.cargo/config.toml` via toml_edit

Status: **Complete (2026-06-27)** — W1–W5 + tail W1/W2 done; every sync-driven consumer
patch lives in `.cargo/config.toml`. Entry/Node-split fixtures collapsed + migrated via
issue 0100. Remaining Cargo.toml patches are all outside the sync model (root / no-pkgxml /
templated / west) and intentionally left. Issue 0094 resolved at W4. · Resolves
[issue 0094](../issues/0094-ws-sync-toml-line-scanner-fragility.md)
· RFC-0023/0024 (binding layouts), RFC-0026 (workspace model).

> **Progress (2026-06-24).**
> - **W1** `8a383bb80` — `write_patch_config`/`render_patch_config` (toml_edit DOM, per-key
>   `# nros-managed` marker, atomic write) + ported tests.
> - **W2** `aaba1a85f` — read-side (`extract_consumer_registry_nros_deps`) on toml_edit.
> - **W3** `c29af6103` — migrated the 6 workspace examples (patch → root/per-member
>   `.cargo/config.toml`); flipped `write_patch_block` live.
> - **W4** `739e44da2` — deleted the dead Cargo.toml line-scanner
>   (`render_patch_block`/`splice_patch_block`/`extract_patch_table`/`RenderedBlock` + tests);
>   `run_clean`/`run_doctor` read `.cargo/config.toml`; book/RFC-0040 updated. **0094 A–F dead.**
> - **W5a** `f03cb442d` — promoted `nros ws sync` → top-level **`nros sync`** (serves both
>   layouts); `ws sync` kept as a hidden deprecated alias; `generate-rust` stays the codegen
>   primitive; scripts/just/book + the `cargo.sh` capability probe re-pointed.
> - **W5b** `432d136e3` — migrated 88 standalone example packages (Cargo.toml patch →
>   `.cargo/config.toml`). Verified: native build, multi-lane `cargo metadata` resolve,
>   idempotent re-sync.
> - **W5b (classifier)** `e7cf17106` + `03c7a89d7` — `nros sync` now recognizes a
>   node-with-msgs (defines msgs inline AND a Rust node) as a patch consumer
>   (`WsPkg::needs_patch_authority` drops the wrong `!is_msg_pkg` guard), and single-pkg mode
>   is dependency-aware (`emitted_msg_dep_closure`) so a node's unconsumed/mis-named
>   self-codegen crate no longer lands a broken patch path. Migrated `native/custom-msg` +
>   `zephyr .../talker-aemv8r` (build-verified native; +3 unit tests).
> - **W5b (embedded build verify)** 2026-06-24 — the **threadx-linux** lane is build-verified:
>   `just threadx_linux::build-examples` rust stage builds all 6 standalone examples
>   (talker/listener/service-{client,server}/action-{client,server}) + the logging-smoke
>   fixture green via `nros sync` + cargo with the migrated `.cargo/config.toml` patches
>   (full ThreadX + NetX Duo + zenoh-pico; EXIT=0, zero errors). Plus native `cargo build`.
>   So phase acceptance "rebuild native + ≥1 embedded lane" is met with real builds; the
>   other embedded lanes stay `cargo metadata` resolve-verified. (The C-workspace fixture
>   stage has an unrelated pre-existing cmake `mv .args.tmp` glitch — not patch-related.)
> - **tail W1** `1cc771279` — migrated the 8 remaining single-pkg `MANAGED+pkgxml` consumers
>   (6 `nros-bench` bins, `cdr-roundtrip-qemu`, `multi-package-workspace/pkg_rust_publisher`):
>   Cargo.toml patch → `.cargo/config.toml`; all resolve rc=0, qemu bins build green.
> - **tail W2** `cebc03249` — `local-msg-package/rust_consumer` via WORKSPACE-mode sync (it is a
>   colcon template workspace; single-pkg sync missed the local `local_msgs`/`extra_msgs`
>   siblings — syncing the root patches all of them). rc=0.
> - **Entry/Node-split fixtures (qemu-arm-baremetal `*`, stm32f4) — DONE** via
>   [issue 0100](../issues/0100-baremetal-standalone-examples-split-into-sibling-node-pkg.md)
>   (W1–W7): collapsed to self-contained crates, then auto-migrated through single-pkg sync.
>
> **End state.** Every sync-driven consumer patch now lives in `.cargo/config.toml`. The
> `[patch.crates-io]` tables that remain in a `Cargo.toml` are all OUTSIDE the sync model and
> intentionally left: the workspace **root** `Cargo.toml` (not an example); **px4 xrce** ×3 +
> `nros-tests/bins/declarative-safety-listener` (no `package.xml` — hand patches, not
> sync-managed); `nros-tests/fixtures/orchestration_tiers_freertos` (a `@NANO_ROS_ROOT@`-templated
> fixture, substituted at build time); and `examples/workspaces/rust/src/zephyr_entry` (west-built,
> workspace-excluded). None is written by `nros sync`, so none reopens the issue-0094 class.

> **Goal.** Stop `nros ws sync` from editing a consumer `Cargo.toml`. It writes its
> `[patch.crates-io]` into a **`.cargo/config.toml`** instead, using a format-preserving
> `toml_edit` DOM (per-key `# nros-managed` marker). **Same authority/granularity as today** —
> the two-layout model (shared root for Rust-only workspaces; per-Rust-member for mixed) is
> **unchanged**. With no manifest edit left, issue 0094's whole A–F class is structurally
> impossible.
>
> **Scope correction (2026-06-23):** an earlier draft proposed unifying every workspace onto
> independent per-package topology (dropping the root cargo `[workspace]`). **Dropped** — it
> contradicts the deliberate shared-workspace binding model just landed in `3f07dd9f7`
> (RFC-0023/0024) and discards the clean shared layout that is natural + correct for Rust-only
> workspaces. This phase changes only **where** (`.cargo/config.toml`) and **how** (toml_edit)
> the patch is written — not topology, granularity, or the binding layout.

## Why

### 0094 — the writer is a line scanner
`ws sync` rewrites the consumer `Cargo.toml`'s `[patch.crates-io]` with a line-based text
scanner + a `BEGIN…END` managed-region marker (`packages/cli/nros-cli-core/src/cmd/ws.rs`:
`render_patch_block` / `splice_patch_block` / `extract_patch_table` / `strip_managed_block` /
`extract_consumer_registry_nros_deps`). TOML-equivalent shapes break it (0094 A–F). A/B/C were
hardened in `68e167275`; the root cause — editing a hand-authored manifest with a non-parser —
remains. The fix is to stop editing the manifest at all.

### Topology follows language composition (the model to PRESERVE)
Surveyed across `examples/workspaces/*`:
- **Rust-only workspace** (`rust`, `ws-{lifecycle,params,realtime,safety}-rust`) — has a root
  cargo `[workspace]`; **shared root `generated/`**; **one root `[patch.crates-io]`** for all
  members. `ws sync` over the root.
- **Has C/C++ members** (`mixed`, `c`, `cpp`) — **no** root cargo `[workspace]` (C/C++ aren't
  cargo packages); **per-member `generated/`**; patch only in each **Rust** member (`mixed`'s
  `rust_heartbeat_pkg`), per-member; C/C++ members need no cargo patch.
- **Standalone** (`examples/<plat>/<lang>/…`) — per-pkg `generated/`, **hand-curated** per-pkg
  patch, `nros generate-rust` (codegen only; never rewrites the patch). **Out of `ws sync`'s
  patch-writer scope.**

`ws sync`'s patch granularity is **already correct** — it mirrors the authority
(`find_patch_authority`): root for Rust-only workspaces, per-Rust-member otherwise. This phase
keeps that.

### `.cargo/config.toml [patch]` works + needs no root manifest — proven
- `paths` override is **out** (cargo docs: crates.io-published only; nros crates aren't).
- `[patch.crates-io]` in `.cargo/config.toml` is stable (cargo 1.96; the repo already uses it
  for the NuttX `libc` build-std patch) and **resolves unpublished `nros-core = "*"` with a
  clean manifest** (empirical: present → resolve `rc=0`; removed → `no matching package`).
- It is auto-discovered up the directory tree → plain `cargo build` works, with or without a
  root Cargo.toml (proven: ancestor config patch applies to a package built beneath it). So it
  fits **both** layouts — root config for the workspace, per-member config for mixed.

## Decision
1. `ws sync` writes `[patch.crates-io]` only into **`.cargo/config.toml`**, at the **authority
   dir it already resolves** (root for Rust-only workspace; the Rust member dir for mixed).
   Consumer `Cargo.toml` is **never** modified by sync.
2. **All TOML editing via `toml_edit`** (format-preserving DOM) — both the config write and the
   `Cargo.toml` dep-scan read. No line scanners.
3. **No topology / binding-layout change.** Shared-vs-per-member, the `[workspace]` presence,
   `generated/` placement — all unchanged through W1–W4. Standalone stays `generate-rust` /
   hand-curated for W1–W4; **W5 then unifies** standalone + workspace onto one command
   (`ws sync`) + one managed patch model (`.cargo/config.toml`), once the toml_edit writer is
   proven — superseding `3f07dd9f7`'s standalone routing. Binding layouts remain per-layout.

## Design

### Write — `.cargo/config.toml` DOM (replaces render/splice/strip)
```rust
let cfg = authority_dir.join(".cargo/config.toml");          // SAME authority as today
let mut doc: DocumentMut = read_or_empty(&cfg).parse()?;
let patch = doc["patch"]["crates-io"].or_insert(implicit_table());  // bare/quoted/dotted = one key
patch.retain(|_, v| !decor_has_marker(v));                   // evict prior nros-managed
for (name, rel) in managed_sorted {                          // generated crates + nros-core/serdes + 220.E/244-E3 extras
    let mut it = InlineTable::new(); it.insert("path", rel.into());
    let mut item = value(it); set_suffix_decor(&mut item, "  # nros-managed");
    patch.insert(&name, item);
}
if patch.is_empty() { doc["patch"].as_table_mut().remove("crates-io"); }   // 0094 F
write_atomic(&cfg, &doc.to_string());                                       // temp + rename (kept)
```
- **Per-key `# nros-managed` decor marker** = ownership (no region). The config table mixes
  sync-owned patches with user content (`[target]`/`[env]`, the hand `libc` patch) — the marker
  is what sync evicts/replaces; user keys + their decor are preserved. Sorted → diff-stable;
  stale generated crates evicted by the marker.
- Atomic temp + `rename` preserved. Create `.cargo/config.toml` (+ `.cargo/`) if absent.

### Read — dep-scan via DOM
`extract_consumer_registry_nros_deps` + `extract_cargo_path_deps` → walk `doc["dependencies"]` /
`["dev-dependencies"]` / `["build-dependencies"]` / `["target"][cfg][kind]` as DOM tables;
inline `name = {version=…}` and explicit `[dependencies.name]` collapse to one shape (0094 B
gone). `package.xml` → codegen → generated-crate-names unchanged (separate XML path).

### Managed-set + path policy — unchanged
Three key sources (generated msg crates; minimum `nros-core`/`nros-serdes`; registry-style
`nros-*`/`cyclonedds-sys` from consumer + generated `Cargo.toml`s) mapped via the static
`nros_crate_path_lookup` table; paths via `pathdiff` from the config's package dir.

### Why all 6 0094 cases die
A (quoted) + B (dotted) → same DOM key. C (region) → no region. D (`"""`) → strings are nodes.
E (path) → serializer escapes. F (empty) → `remove`.

## Work items

- **W1 — config writer (toml_edit) + tests.** New `write_patch_config` (DOM + per-key marker +
  atomic write) behind the existing `write_patch_block` call site. Port the 18 `patch_block`
  tests to assert observable output (one patch table; user keys incl. `libc`/`[target]`
  preserved; managed evicted+reinserted; idempotent; stale-generated eviction). Add: quoted/
  dotted equivalence, `"""`-immunity, empty-set removal.
- **W2 — read-side DOM.** `extract_consumer_registry_nros_deps` + `extract_cargo_path_deps` →
  `toml_edit`. Keep `package.xml` parsing.
- **W3 — MIGRATE the in-tree workspace examples** (move each ws-sync-managed patch
  Cargo.toml → `.cargo/config.toml`; strip the `BEGIN…END` block from the Cargo.toml; deps stay
  `version = "*"`). One-time, via re-running the new `ws sync` per workspace, then committing:
  - `examples/workspaces/rust/` — root patch → `rust/.cargo/config.toml` (merge with the existing
    `[target]`/`[env]`/`libc` config); `src/zephyr_entry/` patch → `zephyr_entry/.cargo/config.toml`.
  - `examples/workspaces/ws-lifecycle-rust/`, `ws-params-rust/`, `ws-realtime-rust/`,
    `ws-safety-rust/` — root patch → `<ws>/.cargo/config.toml`.
  - `examples/workspaces/mixed/src/rust_heartbeat_pkg/` — patch → that pkg's `.cargo/config.toml`.
  - `c/`, `cpp/` — no Rust patch; nothing to migrate.
  - Rebuild native + ≥1 embedded lane (freertos/threadx) per affected workspace to confirm
    resolve; assert no `[patch.crates-io]` remains in any migrated `Cargo.toml`.
- **W4 — delete dead scanners.** Remove `render_patch_block`, `splice_patch_block`,
  `extract_patch_table`, `strip_managed_block`, `is_patch_crates_io_header`,
  `strip_toml_key_quotes`, the `BEGIN/END` constants, `RenderedBlock`. Update `run_clean` /
  `run_doctor` to read `.cargo/config.toml`. Update the scaffold/template generator + any docs
  (book) that mention the BEGIN/END managed block.
- **W5 — unify the command + patch management (standalone ⇄ workspace).** Today standalone goes
  through `nros generate-rust` (codegen only; hand-curated `Cargo.toml` patch) while workspaces go
  through `nros ws sync` (codegen + managed patch) — a split `3f07dd9f7` made *because the patch
  editor was fragile*. W1–W4 remove that fragility (toml_edit DOM into `.cargo/config.toml`), so
  the split's rationale dissolves and leaving it produces **two patch models** (standalone hand
  `Cargo.toml` vs workspace managed `.cargo/config.toml`). Unify:
  - **Rename `nros ws sync` → `nros sync`.** Once it serves both layouts, the `ws`
    (workspace) qualifier is wrong + cumbersome. `nros sync` = "sync generated bindings + the
    cargo patch config to match the declared deps (`package.xml` / `Cargo.toml`)" — for a
    standalone pkg **or** a workspace; it picks single-pkg vs colcon mode by layout (as
    `run_sync` already does). Keep `nros ws sync` as a `#[command(hide=true)]` deprecated alias
    for one release cycle (emit a one-line deprecation note); update `just`/scripts/book to
    `nros sync`. (`nros setup` is taken — toolchain/SDK provisioning, RFC-0014 — so not that.)
  - **`nros sync` is the single user-facing command for BOTH layouts.** It already has
    `single_pkg_mode` (codegen + patch) — a superset of `generate-rust` — so a standalone pkg
    syncs through it and gets its `.cargo/config.toml [patch.crates-io]` managed exactly like a
    workspace member, at per-pkg granularity.
  - **`nros generate-rust` stays the low-level codegen primitive** (`nros sync` calls it; available
    for codegen-only needs — IDE/CI/debug — with no patch side effects).
  - **Binding layouts unchanged** — `nros sync` single-pkg → per-pkg `generated/` (standalone);
    workspace → shared root `generated/`. Only patch *management* becomes uniform.
  - Re-point the orchestration (`scripts/build/regenerate-bindings.sh`) so standalone runs
    `nros sync` (single-pkg) not `generate-rust`; migrate the hand-curated standalone patches
    (`examples/<plat>/<lang>/*`) `Cargo.toml` → `.cargo/config.toml` via a one-time `nros sync`
    pass. **Supersedes `3f07dd9f7`'s standalone routing** (safe now under toml_edit).
  - End-state: every patch — standalone or workspace — lives in `.cargo/config.toml`, managed by
    one command via one toml_edit writer. Large, repo-wide migration → its own verification pass;
    sequence **after** W1–W4 prove the writer end-to-end.

## Sequencing
W1 (writer) → W2 (read DOM) → W3 (migrate + rebuild the workspace examples) → W4 (delete +
docs) → W5 (unify command + management; repo-wide standalone migration — own pass). 0094 is
resolved at W4 (workspaces); W5 brings standalone into the same one-model end-state. Each ships
green `just check` + cli-core unit tests; W3/W5 also rebuild native + ≥1 embedded lane.

## Acceptance
- After a `ws sync`, **no consumer `Cargo.toml` is modified** (grep-clean); the managed patch
  lives only in `.cargo/config.toml`.
- All migrated workspace examples + standalone examples build (native + ≥1 embedded lane) with
  the existing fixture lanes / plain `cargo build`.
- 0094 A–F structurally impossible; issue 0094 resolved (at W4).
- **W4:** binding layout unchanged from `3f07dd9f7` (shared root for Rust-only workspaces;
  per-Rust-member for mixed; standalone per-pkg) — only the patch *file* + *editor* changed.
- **W5:** one command (`ws sync`) + one patch model (`.cargo/config.toml`, toml_edit) for both
  standalone and workspace; `generate-rust` is the codegen-only primitive; binding layouts still
  per-layout.

## Notes / risks
- `toml_edit 0.22` already a `nros-cli-core` dep.
- Blast radius: the canonical patch writer + a one-time migration across the 7 ws-sync-managed
  example manifests (W3). Standalone migration (W5) is the larger, optional churn.
- cargo "patch not used in the crate graph" warning doesn't apply to managed entries (they're
  used) — unlike the intentional `libc` one.
- Keep the static `nros_crate_path_lookup` table + the `package.xml`→codegen path as the SSoT
  for the managed set; only the read/write *mechanism + target file* change.
