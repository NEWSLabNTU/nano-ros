# crates.io Publish-Readiness Audit (Phase 111.B.1 + B.2)

_Date: 2026-05-17. Audit script: `tmp/audit_meta.py`._

## 1. Summary

- **Crates audited:** 35 (workspace members under `packages/{core,zpico,xrce,dds,codegen}/`, excluding `[workspace]`-only manifests, `generated/` message crates, fixture/sample projects, and vendored third-party trees).
- **Fully ready (all 11 fields present, directly or via `workspace = true`):** 26 / 35 (74%).
- **Missing at least one field:** 9 / 35 (26%).
- **Root `[workspace.package]` already provides:** `version`, `license`, `repository`, `homepage`, `documentation`, `readme`, `authors`, `keywords`, `categories`. Missing at workspace level: `description` (per-crate by design), `name` (per-crate by design).

## 2. Per-Crate Gaps

Crates not listed are fully ready.

| Crate | Missing Fields |
| --- | --- |
| `colcon-nano-ros` | `repository` |
| `nros-sizes-build` | `homepage`, `documentation`, `readme`, `authors`, `keywords`, `categories` |
| `nros-rmw-dds` | `homepage`, `documentation`, `readme`, `authors`, `keywords`, `categories` |
| `nros-rmw-dds-staticlib` | `readme`, `keywords`, `categories` |
| `nros-rmw-xrce-cffi` | `homepage`, `documentation`, `readme`, `authors`, `keywords`, `categories` |
| `nros-rmw-zenoh-staticlib` | `readme`, `keywords`, `categories` |
| `zpico-link-ivc` | `repository`, `homepage`, `documentation`, `readme`, `authors` |
| `zpico-serial` | `repository` |

All other crates either inherit from `[workspace.package]` or set the fields explicitly.

## 3. Common Gaps & Recommended Fix

Most gaps cluster around the same handful of fields and are all available via workspace inheritance — they are present in `[workspace.package]` and just need to be opted into per-crate.

Ranked by gap count:

1. **`readme`** — 6 crates. Workspace value already set.
2. **`keywords`** — 5 crates. Workspace defaults available.
3. **`categories`** — 5 crates.
4. **`homepage`** — 4 crates.
5. **`documentation`** — 4 crates.
6. **`authors`** — 4 crates.
7. **`repository`** — 3 crates.

**Recommended action.** For each crate listed in §2, add the inherited form in the `[package]` block:

```toml
homepage.workspace      = true
documentation.workspace = true
readme.workspace        = true
authors.workspace       = true
keywords.workspace      = true
categories.workspace    = true
repository.workspace    = true
```

`description` stays per-crate (one short sentence each). No new workspace-level fields are needed — every gap field is already declared in `[workspace.package]`. `nros-sizes-build` is a build-only rlib used by `const _` probes; safe to add the boilerplate even if it never publishes.

Optional follow-ups: add `rust-version.workspace = true` and `publish = false` to internal-only crates (`nros-sizes-build`, `*-staticlib`, `zpico-link-ivc`) to make publish intent explicit and prevent accidental release.

## 4. Name Availability (crates.io)

All thirteen reserved names returned HTTP 404 from `https://crates.io/api/v1/crates/<name>` — all **AVAILABLE**.

| Name | Status |
| --- | --- |
| `nros` | AVAILABLE (404) |
| `nros-core` | AVAILABLE (404) |
| `nros-serdes` | AVAILABLE (404) |
| `nros-rmw` | AVAILABLE (404) |
| `nros-rmw-zenoh` | AVAILABLE (404) |
| `nros-rmw-xrce` | AVAILABLE (404) |
| `nros-rmw-dds` | AVAILABLE (404) |
| `nros-node` | AVAILABLE (404) |
| `nros-c` | AVAILABLE (404) |
| `nros-cpp` | AVAILABLE (404) |
| `zpico-sys` | AVAILABLE (404) |
| `cargo-nano-ros` | AVAILABLE (404) |
| `nros-cli` | AVAILABLE (404) |

**Total:** 13 / 13 available. Recommend reserving these via a single dummy publish (v0.0.0-reserved) before any public announcement to prevent name-squatting on the headline crates.
