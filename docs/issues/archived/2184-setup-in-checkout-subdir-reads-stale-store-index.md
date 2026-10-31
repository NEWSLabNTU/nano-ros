---
id: 2184
title: "`nros setup` run from a subdirectory of a checkout reads the store's cached index, which is never refreshed, and its refusal names the checkout's gate"
status: resolved
type: bug
area: [cli, installed-sdk]
severity: low
found: 2026-10-31
related: [1304, 2178, rfc-0097, rfc-0103]
resolved: 2026-10-31
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

## Resolution

Resolved 2026-10-31, all three points of the fix direction, in
`packages/cli/nros-cli-core/src/cmd/setup.rs`:

1. **The enclosing checkout is a rung of `resolve_index`'s default.** Between
   "the bare name resolves in the cwd" and the store (RFC-0097 D5) now sits
   `enclosing_checkout_index`: the launcher's `find_monorepo_root` walk from
   the cwd — the same walk `index_workspace` (issue 1304) does for `dest` —
   and the root's `nros-sdk-index.toml` if it holds one. `--index <path>` and
   `$NROS_INDEX_URL` keep outranking it (RFC-0103 D3's arg and env rungs): a
   mirror the operator named is read even inside a checkout. The walk is
   deliberately NOT the full `nano_ros_root` ladder: its last rung is the
   binary's own root, and a released `nros` stages the index into
   `share/nano-ros/` too, so taking that rung would put the shipped copy above
   the store — the coupling D5 removed. A host with no checkout resolves
   exactly as before.
2. **An unreadable cache is never served.** `index_from_store` loads the
   cached copy before answering with it; one that fails to parse or validate
   is treated as `refresh` on both rungs that used to return it (the warm
   cache, and the "cache again" fallback after a failed refresh), the reader
   is told on stderr, and when nothing else answers the error says the cache
   is the broken piece rather than "none cached". `fetch_index`'s rule that an
   unparseable fetch is never cached is unchanged; this is its symmetric half.
   Cost: one extra parse of the ~170 KB index per resolve.
3. **The refusal names the way out for the path's origin.** The four verbs
   that load the resolved index (`setup`, `locate`, `sdk-path`, `sdk-front`)
   go through `load_resolved_index`, which wraps a failed load with
   `index_load_remedy(path, IndexWhence)`: `NROS_INDEX_REFRESH=1` for the
   store's cached copy, `--index <path>` for an explicit one, and
   `scripts/check/check-source-refs.py --write` only for a checkout's own.
   `SdkIndex::validate`'s `submodule`-without-`git`/`ref` message no longer
   tells every reader to run that gate.

Verified by six unit tests in `cmd::setup::tests` (scratch checkout + scratch
store, `file://` fetches, no network, no process env) covering the walk from a
subdirectory, the rung order with and without an enclosing checkout, the
`$NROS_INDEX_URL` and `--index` precedence, the refetch of an unreadable
cache, the offline fallback past one, and the per-origin remedy text — and by
hand with the rebuilt in-tree `nros`, `$NROS_HOME` pointed at a scratch store
holding a deliberately unloadable `fetch/nros-sdk-index.toml`, `NROS_OFFLINE=1`:
from `tmp/scratch2184/deeper/` inside the checkout `nros setup --list` listed
the checkout's index; from a directory outside every checkout the same command
printed "cannot be read by this `nros` (...); not serving it" and refused naming
the cache; with `NROS_INDEX_URL=file://<checkout>/nros-sdk-index.toml` it
replaced the cache (166,641 bytes, the checkout's) and listed. The real store's
index was not touched.

Not done here, on purpose: `locate_index` (the BUILD rung, `fetch = false`)
keeps its cwd → workspace → `$NROS_WORKSPACE` → store order; its callers pass
the SDK root they already resolved, so a build inside a checkout reaches the
checkout's index that way. `nros doctor` still says nothing about the cache's
age — it no longer needs to for correctness, since an unreadable copy is now
replaced rather than served.
