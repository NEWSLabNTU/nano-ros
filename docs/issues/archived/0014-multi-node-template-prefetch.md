---
id: 14
title: examples/templates/multi-node-workspace missing generated dir in broad build
status: resolved
type: bug
area: build
related: [phase-226]
resolved_in: templates excluded from prefetch
---

Surfaced by Phase 226.F: `build-all-jobserver.sh` failed with `failed to
load source for dependency builtin_interfaces` — the template path-deps a
gitignored `generated/builtin_interfaces/Cargo.toml` (only `nros ws sync`
materialises it), so the standalone-manifest prefetch
(`nros_cargo_fetch_standalone_manifests()` in `scripts/build/cargo.sh`)
hard-failed on `cargo fetch`.

Fixed: templates are copy-out recipes built by no fixture row or broad-build
recipe, so they should not be cache-warmed. Added `-g '!examples/templates/**'`
to the prefetch glob (mirrors the existing `examples/zephyr/**` exclusion).
