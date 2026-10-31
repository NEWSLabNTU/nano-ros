---
id: 2186
title: "On a GCC 16 / libclang 23 host `check fast` fails five gates, and one of them rewrites two committed files on every `pre-push`"
status: resolved
type: bug
area: [gates, ci, abi]
severity: medium
found: 2026-11-01
resolved: 2026-11-01
related: [1043, 1464, 0196, 0986]
---

## What

On the Arch host (GCC 16.2.1, libclang 23.1.3, a system `/usr/bin/sccache`) the
`pre-push` hook's `just check fast` failed five gates on a commit that is green
inside the Ubuntu `ros2` distrobox (GCC 11, libclang 14). None of the five is
about a change: each is a gate whose verdict depends on something about the
HOST that it did not account for. Measured on `origin/main` 783eb1465e:

| gate | host verdict | cause |
| --- | --- | --- |
| `abi-bindings` | FAIL, **and the worktree is dirty after** | the gate ran `gen-abi-bindings.sh` in WRITE mode and then `git diff`ed; libclang 23 emits more doc comments, so it rewrote `packages/rmw/cffi/src/generated.rs` (+40 lines) and `packages/platform/nros-platform-cffi/src/generated.rs` (+8) |
| `comment-names` | FAIL | reads the rewritten file: `INT64_MIN` now appears in a doc string |
| `retired-knob-spellings` | FAIL | reads the rewritten file: `ZPICO_MAX_SESSIONS` now appears in a doc string |
| `cpp-hosted-minimal-libcpp` | FAIL (selftest) | the nested-comment control matched GCC 13's quoting `"/*" within comment`; GCC 16 prints `‘/*’ within comment` |
| `rustc-wrapper-staticlib` | FAIL | the "no sccache" probe case put `/usr/bin:/bin` on PATH and found the host's real sccache |

(The other nine reds in that run were a stale in-tree CLI in the checkout,
unrelated: they clear after `just setup-cli`.)

## Root causes

### 1. A gate wrote tracked files (the class)

`check-abi-bindings` regenerated in place and diffed. Its write was
write-if-changed, so where the host's libclang agrees with the committed bytes
nothing was written — which is every place this repo had looked (the box, the
`ci-base` image). It writes exactly when it fires. The rewrite then dirtied the
developer's tree, re-staled every fixture keyed on those files, and made
`comment-names` and `retired-knob-spellings` red reading text nobody committed.

Sweep for siblings: `git grep -n "git diff --exit-code\|git diff --quiet" --
just scripts` finds only this gate; the other gates that run a generator either
use `--check` (compare in memory) or write a GITIGNORED artifact
(`pool-inventory`, `config-surface`, `issue-index`, `book-links`), which is not
a tracked file.

**Why `check-hook-repo-side-effects` did not catch it (0196's reach gap).** That
gate runs the hook with its `just check fast` arm SKIPPED — running it there
would recurse — on the stated grounds that the lane is "separately gated". It
was not. And a probe there could not have reproduced it anyway: the write
happens only on a host whose tools disagree with the committed bytes.

### 2. The bindgen output depends on the libclang major — for doc comments

CLAUDE.md and `ci/docker/ci-base/Dockerfile` say "the bytes do not depend on
the libclang major (12 and 14 agree)". True for 12 vs 14; **false for 23**:
libclang 23 attaches the `/** */` comment above a `#define` to the macro, and
bindgen (0.72.1, the pin) emits it as `#[doc = ...]`. Measured: every
difference is an added doc attribute — with doc attributes stripped, all three
surfaces are byte-identical to the committed files. So the output legitimately
depends on the libclang, and only in comments.

### 3. `cpp-hosted-minimal-libcpp`: a test artifact, not a header defect

The selftest's control (`/** see include/nros/*.hpp */`) did fail on GCC 16 with
`-Werror=comment` exactly as intended; the gate then pattern-matched the
diagnostic's PUNCTUATION and rejected it. All 75 headers compile on GCC 16.

### 4. `rustc-wrapper-staticlib`: the gate's own PATH, not host config

The shim looks for "the first `sccache` on PATH that is not this file". The
probe's "no sccache installed" case built `PATH=<shim>:<fakes>:/usr/bin:/bin`,
so on a host with `/usr/bin/sccache` the real sccache ran the probe's fake
`rustc` (a shell script) and died "Failed to launch subprocess for compiler
determination". The box has no system sccache, so it passed there. Nothing is
wrong with the host's sccache.

## Fix

- `scripts/gen-abi-bindings.sh --check` generates into a temp dir and compares;
  `check-abi-bindings` calls it and never writes the tree. Write mode no longer
  stages `.tmp` files beside the committed ones either.
- The libclang major the committed bytes come from is recorded
  (`LIBCLANG_REF_MAJOR=14`) and read from `bindgen --version --verbose`.
  `--check` on another major compares with doc attributes stripped: a CODE
  difference is a FAIL (really stale, the diff shown is code-only); a doc-only
  difference is **NOT VERIFIED** through the `nros_check_skip` ledger, naming
  expected vs found (issue 1043's third outcome). Write mode refuses a foreign
  libclang (the committed bytes would flap); `NROS_ABI_BINDINGS_ANY_LIBCLANG=1`
  overrides. A bindgen that is on PATH but cannot start (a host binary reached
  through a shared `$HOME`) is NOT VERIFIED too, instead of a silent
  `set -e` exit. The doc-only classifier has a negative control on every
  `--check` run (the script left the selftest ratchet baseline).
- `scripts/build/run-gates-parallel.sh` (fast lane, both parallel and serial):
  snapshots (path, blob hash) of every tracked file differing from HEAD before
  and after the gates and FAILS the lane on any change, naming the files. This
  is the "separately gated" the hook gate assumed; its comment now says so.
- `cpp-hosted-minimal-libcpp` matches `'/*' … within … comment` — GCC 13, GCC 16
  (UTF-8 and `LC_ALL=C`) and clang spellings.
- `rustc-wrapper-staticlib` drives the shim with a PATH of only the fakes plus
  symlinked `bash` and `dirname`.

## Evidence

- Before (host): the five gates above red; `git status` after the run shows the
  two `generated.rs` files modified.
- After (host): `abi-bindings` reports `[SKIPPED] abi-bindings: NOT VERIFIED —
  bindgen here uses libclang 23, expected 14: … differ only in doc comments
  (code matches)`; the other four pass; `git status` clean after the run.
- Negative controls: a committed `generated.rs` with one constant changed FAILS
  `--check` on the host (libclang 23, code-only diff shown) and in the box
  (libclang 14), and the file's sha256 is unchanged after; a throwaway gate that
  appends to a tracked file makes `run-gates-parallel.sh` fail naming it.

## Not done here

CLAUDE.md's "12 and 14 agree" line and the same sentence in
`ci/docker/ci-base/Dockerfile` are now incomplete (23 differs in doc comments);
left for the maintainer, since editing the Dockerfile rebuilds the CI image.
