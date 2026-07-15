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

## What closing this needs

Not a repo-mechanical fix — it needs release-owner action:

1. An Espressif component-registry account/token for the org (maintainer-held;
   agents must not create or hold publish credentials).
2. A first manual publish following `docs/release/registry-publishing.md`
   (verify the doc survives contact with the real registry; fix drift).
3. A CI lane (tag-triggered) that re-publishes on release, plus a smoke test
   that `idf.py add-dependency` against the registry resolves and builds the
   ESP-IDF example.

## Non-goals

- PlatformIO registry publish (#171 D4 — future work, manifest stays in-tree).
- crates.io / prebuilt binaries (#171 D2 rules them out).
