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
related: [issue-1066, issue-1356, issue-1453, phase-505]
---

**Home: [phase-505](../roadmap/phase-505-ci-lanes-reach-and-infra-robustness.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

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

## Second sighting, 2026-10-14: in the merge group, as an ejection

The same error shape ejected PR #2166 (a docs-only change filing issue 1891)
from the merge queue. Merge-group run **38507231306**, job **115667400933**
`check / fast (every event)`:

```
check-fast (parallel): 1 of 447 gate(s) FAILED
===== FAIL (api-parity, rc=1, 51845ms) =====
error: failed to get `pastey` as a dependency of package `nros v0.6.0 (…/packages/api/nros)`
  download of pa/st/pastey failed
  [28] Timeout was reached (Connection timed out after 30002 milliseconds)
```

This time the dependency is `pastey` instead of `ignore`, so the crate the error
names is unrelated to the diff, the same as before. Two things are new:

- **It now costs a batch, not a re-run.** On a pull request the remedy was
  re-running one job. In the merge group, `grouping_strategy` is ALLGREEN, so
  the red gate removed the PR from the queue (`RemovedFromMergeQueueEvent`
  reason `failed_checks`, 03:49:29Z). It was re-queued afterwards.
- **The "not yet frequent" caveat above has weakened.** There are now two
  occurrences, twelve days apart, on two different events. That is still not a
  rate, but the case for remedy 1 (retry) has grown. Remedy 1 is cheap and does
  not hide a real parity mismatch.

## Third sighting, 2026-10-14: a different site, and on main's push gate

Push gate **38654300841** for main `67e29e9fa`, job **116165031036**
`check / fast (every event)`, on a GitHub-hosted runner, failed in step
`Build the in-tree nros CLI (every event)`, before any gate ran:

```
error: failed to download from `https://static.crates.io/crates/minijinja/2.21.0/download`
Caused by:
  [7] Could not connect to server (Failed to connect to static.crates.io:443 after 0 ms: Could not connect to server)
error: recipe `setup-cli` failed with exit code 101
```

So the dependency on a reachable crates.io is not specific to `api-parity`:
`setup-cli` has it too, and it runs in every lane before `check fast`. The
same day also had a DNS failure for `github.com` in
`submodule-commits-reachable` (merge-group run 38641874951, ejecting #2218).
That makes three network-only reds in one day across three sites.
`after 0 ms` is a connection refused outright, which is the case
`CARGO_NET_RETRY` (remedy 1) handles. The case for setting it once, at the
job level so every cargo invocation inherits it, is now stronger than the
case for fixing `api-parity` alone.

## Fourth sighting, 2026-10-16: post-submit `dep-chain`, an HTTP/2 framing error

Post-submit run **38830050185** for main `25847c111` (#2288), job
**116747973225** `dep-chain (7 board×rmw cells)`. Six cells passed. The
`qemu-armv7a-nuttx:zenoh` cell failed because `cargo tree` could not reach
crates.io:

```
[FAIL] cargo tree did not resolve (default features), exit 101
  error: failed to get `memchr` as a dependency of package `quick-xml v0.41.0`
  ...
  unable to update registry `crates-io`
  download of me/mc/memchr failed
  curl failed
  [16] Error in the HTTP2 framing layer
```

That is a fourth site (after `api-parity`, `setup-cli` and
`submodule-commits-reachable`) and a third failure shape (after a refused
connection and a DNS failure). `CARGO_NET_RETRY` covers this one too: cargo
retries a failed HTTP transfer, and an HTTP/2 framing error is one.
