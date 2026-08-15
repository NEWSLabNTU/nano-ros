---
id: 600
title: "`check-submodule-pinned-locks` blames a moved submodule pointer for what is an uncached crate under `--offline`"
status: open
type: bug
area: build
related: [issue-0560, issue-0359, issue-0378]
---

## What happens

`just ci` stopped with:

```
[FAIL] 1 lock(s) pinned by a submodule manifest no longer resolve:

  packages/cli/nros-launch-resolve
      error: failed to download `hermit-abi v0.5.2`

      Caused by:
        attempting to make an HTTP request, but --offline was specified

  The submodule pointer moved and the lock did not follow (issue 0560).
  Update it the sanctioned way — never a bare `cargo generate-lockfile`:
      just lock-update "" "" <leaf-dir>
  then REVIEW the diff: added/removed packages are a dependency change,
  which is expected when a pinned tag moves, but should be seen.
```

The diagnosis is wrong, and the prescribed remedy would have caused harm.

## Why it is wrong

`hermit-abi v0.5.2` **is** in the lock, with its checksum:

```
packages/cli/nros-launch-resolve/Cargo.lock:530
    name = "hermit-abi"
    version = "0.5.2"
    source = "registry+https://github.com/rust-lang/crates.io-index"
    checksum = "fc0fef456e4baa96da950455cd02c081ca953b141298e41db3fc7e36b1da849c"
```

The lock is byte-identical to `origin/main`'s (`git diff origin/main...HEAD --
packages/cli/nros-launch-resolve/` is empty). Nothing about the pointer or the
lock is stale. The crate was simply **not in this host's cargo registry cache**,
and the gate resolves with `--offline`.

```
$ cargo fetch --locked
  Downloaded hermit-abi v0.5.2
  Downloaded wasm-bindgen v0.2.126, js-sys v0.3.103, zerocopy-derive v0.8.55, …
```

Seven crates, all platform-irrelevant on Linux — which is why
`just setup-launch-resolve` had **built the binary successfully minutes
earlier**: the build never needs them, only whole-graph resolution does. After
the fetch the gate passes with the lock untouched.

## Why the misdiagnosis matters

The message does not merely mislabel a cause — it prescribes `just lock-update`,
i.e. **re-resolving a lock that was correct**. `Cargo.lock` is "a promise that
someone else's build resolves what yours did"; the repo has been burned twice by
unintended lock churn (issues 0359, 0378 — one bare `cargo generate-lockfile`
moved 5388 lines across 26 leaf locks as a "cleanup"). A gate that tells an
operator to rewrite a good lock, in the imperative, is pointed at exactly that
trap. An agent following instructions literally will do it.

Two distinct conditions are being conflated:

| condition | true cause | correct remedy |
| --- | --- | --- |
| lock does not satisfy the submodule's manifest | pointer moved, lock did not follow (0560) | `just lock-update` |
| lock names a crate this host has not cached | offline resolution, cold cache | `cargo fetch --locked` |

Only the first is issue 0560. The second is a host state and says nothing about
the lock.

## Direction

1. **Separate the two.** The cargo error already distinguishes them: an
   `--offline` download failure carries "attempting to make an HTTP request, but
   --offline was specified", while a genuine mismatch reports an unsatisfiable
   dependency or a missing manifest entry. Match on that and print the matching
   remedy.
2. **Never print `lock-update` for the offline case.** For a cold cache the
   remedy is `cargo fetch --locked` in that leaf, which changes no tracked file.
3. Consider whether the gate should fetch on a cold cache itself rather than
   failing — it is a resolution check, not a network policy — or at minimum say
   "this host has not cached the crates this lock names" so the reader is not
   sent to inspect a submodule pointer that never moved.
4. Sweep for siblings: any other gate that resolves `--offline` and attributes
   every failure to a single cause has the same defect.

## Evidence

Observed 2026-08-15, `wip/feature-contract` rebased onto `6fd06fc75`. The branch
touches no file under `packages/cli/nros-launch-resolve/`, so `origin/main`
reproduces it on any host with a cold cache for those seven crates.
