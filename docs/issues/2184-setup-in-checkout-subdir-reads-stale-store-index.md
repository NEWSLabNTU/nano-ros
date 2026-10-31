---
id: 2184
title: "`nros setup` run from a subdirectory of a checkout reads the store's cached index, which is never refreshed, and its refusal names the checkout's gate"
status: open
type: bug
area: [cli, installed-sdk]
severity: low
found: 2026-10-31
related: [1304, 2178]
---

## What

Measured 2026-10-31 on a developer host, inside this checkout, from
`tmp/nx_rv/` (a scratch directory under the repo root), with the in-tree
`nros` on `PATH`:

```text
$ nros setup rv-virt-nuttx --rmw zenoh
Error: invalid SDK index /home/aeon/.nros/fetch/nros-sdk-index.toml

Caused by:
    source 'cyclonedds-src' has `submodule` but no `git`/`ref` — the pin must be readable without a checkout. Run `python3 scripts/check/check-source-refs.py --write`
```

Three things are wrong at once:

1. **The index it read is not the checkout's.** `cmd::setup::resolve_index`
   takes the default name, finds no `nros-sdk-index.toml` in the cwd, and
   goes straight to `store_index` — the store's `fetch/` copy (RFC-0097 D5).
   The checkout two directories up is never consulted, although
   `index_workspace` (issue 1304) does walk to it for `dest` resolution. So
   a contributor in any subdirectory of the repo provisions from whatever
   `~/.nros/fetch/nros-sdk-index.toml` holds.
2. **That copy is never refreshed on its own.** The cache rung wins
   "unless `refresh`", and refresh is only `$NROS_INDEX_REFRESH`. This
   host's copy was dated 2026-10-05, 26 days behind `main`, and it predated
   `check-source-refs` adding `git`/`ref` to every submodule source — so the
   current CLI refused the index its own store had cached. Nothing ages the
   cache, nothing compares it with the checkout, and `nros doctor` says
   nothing about it.
3. **The remedy is aimed at the wrong file.** "Run
   `scripts/check/check-source-refs.py --write`" is the fix for the
   CHECKOUT's index, which was already correct; the file at fault is the
   cached copy, and the way out (`NROS_INDEX_REFRESH=1`, or `--index
   <checkout>/nros-sdk-index.toml`) appears nowhere in the message.

The installed track is not affected: a released `nros` on a host with no
checkout has exactly one index to read. The hazard is a contributor's
host, where the store accumulates (issue 0500's shape, for the index).

## Fix direction

* `resolve_index`'s default rung should consult the nano-ros root the cwd
  is inside — the same `nano_ros_root::resolve` ladder `index_workspace`
  already uses — before the store cache, so a checkout wins for a
  contributor wherever they stand in it. That is the order RFC-0103 D3
  states for every other resource ("checkout" is a rung; a stale store copy
  outranking it is the 0500 class).
* A cached index that this binary refuses to LOAD should trigger the
  refresh rung rather than a refusal: `fetch_index` already refuses to
  cache what it cannot parse, so the symmetric rule is "do not keep serving
  what you cannot parse".
* The refusal names the path it read; it should also name the way out for
  that path's origin (refresh for the store copy, `--index` for an explicit
  one), and only name `check-source-refs.py --write` when the index IS a
  checkout's.

## Found while

Smoke-building the Rust `rv-virt-nuttx` workspace for issue 2178's probe
track. Worked around with
`NROS_INDEX_REFRESH=1 NROS_INDEX_URL=file://<checkout>/nros-sdk-index.toml`.
