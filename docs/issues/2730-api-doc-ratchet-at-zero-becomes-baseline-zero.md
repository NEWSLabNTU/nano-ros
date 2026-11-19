---
id: 2730
title: "The API-doc id ratchet is empty: make public API docs a plain zero rule"
status: open
type: cleanup
area: [docs, gates]
severity: low
found: 2026-11-19
related: [2581, 2554, 2719]
---

## What

Issue 2581's sweeps and issue 2554's regeneration took the public-API-doc
ids to zero, and `.config/api-doc-ids-ratchet.json` is now `{}`. An empty
ratchet already holds every file at zero, so nothing can regress. But the
ratchet machinery still runs for a rule that no longer needs it:
`api_doc_hits` goes through `ratchet_problems`, `--update-ratchet` still
rewrites the file, and the check's docstring describes a count that "may
only fall".

## Plan

Make public API docs a baseline-zero subject like the others (issue 2443's
path): report every id found, with no ratchet file. Then delete
`.config/api-doc-ids-ratchet.json`, take `api_doc_hits` off the ratchet, and
update the docstring. `ratchet_problems` stays, because issue 2719's
user-text ratchet still uses it.
