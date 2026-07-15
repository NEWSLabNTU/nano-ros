---
id: 198
title: "ESP-IDF component-registry publish has never been executed and has no CI"
status: open
type: enhancement
area: release
related: [issue-0171, phase-288]
---

## Problem

The one distribution surface #171 left open after phase-287/288: the ESP-IDF
component manifest (`integrations/nano-ros/idf_component.yml`) works via
path/git consumption, and `docs/release/registry-publishing.md` documents the
Espressif Component Registry publish — but the publish has **never been
executed** and there is **no CI** for it (the doc says so explicitly). A user
searching the registry will not find nano-ros; only the documented
path/git route works.

Carved out of #171 (its D4 decision narrowed the registry work to ESP-IDF —
PlatformIO stays in-tree but deliberately unpublished, Arduino is dropped) so
the umbrella issue can close on the completed source-distribution model.

## Findings (2026-07-15 — dry-run with real tooling)

Checked against a provisioned ESP-IDF 5.3 workspace (`just esp_idf setup`,
idf-component-manager / compote 2.4.10):

1. **The component is STRUCTURALLY UNPUBLISHABLE as laid out.** A dry-run pack
   (`compote component pack --project-dir integrations/nano-ros --name nano-ros`)
   produces a 3.4 KB archive containing exactly `CMakeLists.txt`,
   `Kconfig.projbuild`, `idf_component.yml` — the shell only, zero runtime. The
   shell's `set(_nros_root "${CMAKE_CURRENT_LIST_DIR}/../..")` escapes the pack
   root; installed from the registry it would resolve into the consumer's
   `managed_components/` parent and break unconditionally. Publishing therefore
   needs a DESIGN decision first, not just credentials:
   - (a) move the manifest to the repo root with a `files:` filter so the whole
     source tree ships in the archive (consistent with #171 D2 bundled-source;
     check registry size limits and the pack time), or
   - (b) keep the documented path/git consumption as the only ESP-IDF route and
     `wontfix` the registry publish.
2. **Doc drift:** `docs/release/registry-publishing.md`'s reference command
   `idf.py upload-component` is DEPRECATED in IDF 5.3 ("will be removed in
   future versions"); the canonical flow is `compote component upload`
   (`compote component pack` for a credential-free dry-run). Fixed in the doc
   alongside this note.
3. **Version drift (minor):** the manifest pins `version: "0.1.0"` while the
   workspace is at 0.5.0 — a first publish should decide the component's
   version source of truth.

## What closing this needs

Not a repo-mechanical fix — it needs release-owner action:

0. The layout decision above ((a) pack-the-tree vs (b) wontfix) — blocks
   everything else.
1. An Espressif component-registry account/token for the org (maintainer-held;
   agents must not create or hold publish credentials).
2. A first manual publish via `compote component upload` (the doc's command
   drift is fixed; verify the rest survives contact with the real registry).
3. A CI lane (tag-triggered) that re-publishes on release, plus a smoke test
   that `idf.py add-dependency` against the registry resolves and builds the
   ESP-IDF example.

## Non-goals

- PlatformIO registry publish (#171 D4 — future work, manifest stays in-tree).
- crates.io / prebuilt binaries (#171 D2 rules them out).
