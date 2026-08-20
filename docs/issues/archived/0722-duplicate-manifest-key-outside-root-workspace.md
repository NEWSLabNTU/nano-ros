---
id: 722
title: "A duplicate key in a board manifest broke every nested workspace while root `cargo metadata` stayed green"
status: resolved
type: bug
severity: high
area: build
related: [issue-0708, issue-0710]
resolved_in: "2026-08-20 — `check-manifests-parse`"
---

# 0722 — a manifest no root claims can be broken and read as healthy

`packages/boards/nros-board-esp32-qemu/Cargo.toml` ended 2026-08-20 with
`nros-log` declared **twice** in one `[dependencies]` table (lines 15 and 80).
Cargo does not merge or warn — it refuses the file:

```
error: duplicate key
  --> packages/boards/nros-board-esp32-qemu/Cargo.toml:80:1
```

## How two copies survived a dedup

Three commits touched it the same afternoon:

| | | |
| --- | --- | --- |
| `1293d9a9b` | 13:27 | `fix(#708)` — added a key at what is now line 80 |
| `cea413717` | 13:29 | `fix(#708)` — the same defect, again |
| `6834eb7dc` | 13:59 | `fix: drop my duplicate nros-log key …` — removed **one** |

The dedup deleted a third copy and left the two that remain. Each author was
looking at the key they had just written, not at the table.

## Why nothing caught it

The failure surfaces in whichever workspace **claims** the crate, and this tree
has many roots — the repo root, `packages/cli`, and each nested example
workspace. The root workspace does not include this board:

```
$ cargo metadata --no-deps >/dev/null && echo OK      # from the repo root
OK
```

So the tree read as healthy from the place anyone would look. It was
`examples/workspaces/rust` that died on it, four frames up and naming a
different crate entirely:

```
error: failed to load manifest for workspace member `…/examples/workspaces/rust/src/esp32_entry`
Caused by:
  failed to load manifest for dependency `esp32_entry_nros_selection`
```

which is how it took out `just format` — a recipe nobody expects to be a
manifest check.

## Fix

The duplicate itself was removed upstream by `0cb380fce` (issue 0714), which was
fixing a different consequence of the same 0710 change and collapsed the two
declarations into one carrying the new `platform-sink` feature. This issue's
contribution is the gate, because the fix and the gap are separate: 0714 removed
*this* duplicate; nothing yet stopped the next one.

Gated by `check-manifests-parse` (`scripts/check-manifests-parse.py`, on the
`just check` fast line): every `Cargo.toml` tracked by git must parse as TOML.
Nothing about content — this is the syntactic floor that a duplicate key, an
unterminated string, and a mis-nested table all fall through, and it is keyed on
`git ls-files` rather than on workspace membership, which is the property that
failed here.

The manifests are parsed directly rather than shelled out to cargo: 353 files
against one `cargo metadata` each is the difference between a fast-line gate and
a coffee break, and a TOML parser rejects a duplicate key for the same reason
cargo does. Python 3.10 has no `tomllib`, so it uses the repo's established
`tomli` fallback.

## How it surfaced

`just format`, run before committing unrelated work — a recipe nobody expects to
be a manifest check, which is the point: the defect was found by accident and by
a command chosen for something else.
