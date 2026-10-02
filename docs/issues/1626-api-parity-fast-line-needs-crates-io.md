---
id: 1626
title: "`check-api-parity` shells out to `cargo doc`, so the ONE required status
  check fails on a crates.io hiccup — with an error naming a dependency of a
  crate nobody touched, and no retry anywhere in the repo"
status: open
type: bug
area: ci, tooling
severity: medium
found: 2026-10-02
related: [issue-1066, issue-1356, issue-1453]
---

## Measured

PR #1546 (a two-line change to two shell scripts plus one new issue doc) went
red on the required `CI` context. Run **36948856628**, job **110657099726**
`check (fast + PR source gates; …)`, step 14 `just check fast`:

```
check-fast (parallel): 1 of 381 gate(s) FAILED
===== FAIL (api-parity, rc=1, 20956ms) =====
```

Reading past the gate name, the cause is not a parity verdict at all:

```
RuntimeError: cargo doc failed in …/packages/api/nros:
    Updating crates.io index
error: failed to get `ignore` as a dependency of package `tokei v14.0.0`
  Caused by: unable to update registry `crates-io`
  Caused by: download of ig/no/ignore failed
  Caused by: curl failed
  Caused by: [16] Error in the HTTP2 framing layer
```

A transient HTTP/2 failure fetching the crates.io index. The PR's diff is two
`.sh` files and a markdown file.

## Why this is structural and not just a bad minute

**The gate reaches the network by construction.** `scripts/api_parity/
extract_rust.py:113` runs

```python
cmd = ["cargo", "+" + _pinned_nightly(), "doc", "--lib"]
```

with no `--offline`, no `--locked`-only path that avoids the index, and no
retry. On a fresh CI container the cargo registry cache is empty, so `cargo doc`
**must** update the index; there is no cold-cache-free path.

**And `api-parity` is on the fast line, which is the required context.** It came
off `.config/gate-lane-exempt.txt` in issue 1066 deliberately — `gate.yml`'s own
comment records it as the lane's slowest gate at 217,543 ms, 14.1 % of busy — so
it now runs in the `check` job that produces the single required `CI` verdict on
every pull request. That is the right place for it; the consequence is that one
registry hiccup fails the only check that gates a merge.

**There is no retry configured anywhere.** `git grep` for
`CARGO_NET_RETRY`, `net.retry` and `CARGO_HTTP` across `.github/`, `just/`,
`scripts/` and `activate.sh` returns **nothing**, so cargo runs with its
defaults and the gate adds no handling of its own.

**The red is actively misleading**, which is the 1453 class. The error names
`ignore` as a dependency of `tokei`, reached through
`nros-tests` -> `nros-rmw-zenoh` -> `nros-board-freertos` — a chain with no
relation to the diff — and the gate reports it through the same
`[FAIL] api-parity` line it uses for a real ledger mismatch. A contributor's
first reading is "my change broke API parity".

## What this is NOT

- **Not 1356.** That is the reverse case: a build forced `--offline`/`--frozen`
  against a cache that lacks a dependency. Here nothing is offline and the
  fetch is attempted and fails in transit.
- **Not 1066.** Moving `api-parity` onto the fast line was correct and this does
  not argue for reverting it.
- **Not a flake in the code.** Nothing nondeterministic in ours runs here; the
  nondeterminism is the network, and the gate has no opinion about it.
- Not yet shown to be frequent. **One occurrence**, 2026-10-02. The finding is
  the dependency and the absence of a retry, not a rate.

## What would close this

Any one of these, and the choice is a real trade-off rather than obvious:

1. **Retry the transient class** — `CARGO_NET_RETRY` (or `net.retry`) in the
   gate's environment. Cheapest, and covers exactly this error shape, but it
   makes the gate slower to fail when crates.io is genuinely down.
2. **Cache the cargo registry in the job.** The `check` job already caches the
   CLI build and sccache, so the machinery is there; this removes the index
   update on a warm cache and leaves the cold case exposed.
3. **Classify the failure.** Have `rustdoc_json` distinguish "could not reach
   the registry" from "cargo doc rejected the crate" and report the former as a
   skip through the `nros_check_skip` ledger rather than a FAIL. This is the
   only one that fixes the *misleading* half, and it is the one that needs care:
   a skip here must not be able to launder a real parity mismatch.

Re-running the failed jobs cleared it on #1546, so nothing is blocked by this
right now.
