# Phase 300 — CI tooling hygiene: enumeration SSoT + walk/probe fixes

A read-only audit of the check/test/fixture tooling (2026-07-24, during the
full-sweep debt run) found nine pitfalls. The root theme: **directory
walking instead of authoritative enumeration**. `examples/` carries
~640 GB of untracked build trees (per-example cargo `target-*`, cmake
`build-*`, `_deps`), so every unpruned `find`/`grep -r` descends millions
of build-tree inodes; commit `4ca3e06eb` ("enumerate tracked files via git
ls-files, not find") started the fix but missed sibling sites; the
hand-maintained prune ladder is copy-pasted ≥8 places, each a different
incomplete subset.

**Status (2026-07-24): ALL WAVES DONE.** Fire-proofs: an injected probe
crash fails check-fixtures-stale.sh loudly ("a staleness probe crashed");
the new-format signatures re-signed across every family (native, threadx,
freertos via `just freertos build-examples` — the bare script lacks the
recipe env, NROS_PLATFORM_FREERTOS_SRC — nuttx arm + riscv); probe exits 0
clean; `just check` green. Set-equivalence of every ls-files conversion
verified against its find predecessor before the swap.

## Enumeration SSoT (the design decision)

Two distinct enumeration needs, two authoritative sources — no new
manifest file (a second manifest would recreate the drift problem):

1. **Tracked example/source files** (fmt, clippy loops, symbol checks,
   matrix lint): the SSoT is **the git index** — `git ls-files <glob>`.
   O(index), structurally immune to untracked build trees, zero
   maintenance. (`examples/fixtures.toml`'s 333 `dir` records are fixture
   BUILD CONFIGS, cross-platform-duplicated — NOT the tracked-crate list;
   don't drive fmt/check off it.)
2. **Fixture inputs / staleness signatures**: the SSoT is
   **`examples/fixtures.toml` + `scripts/build/fixtures-manifest.py`**
   (already the validated fixture contract). Input enumeration for a
   fixture = `git ls-files -- <dir>` (tracked-only ⇒ `_deps`/`install`
   can never leak into a hash).

Where a walk over UNTRACKED artifacts is genuinely required (clean
recipes), the prune list comes from ONE shared definition.

## Findings (audit 2026-07-24; file:line at audit time)

| # | Sev | Site | Problem |
|---|-----|------|---------|
| F1 | HIGH | `just/qemu-baremetal.just:72,446` | `find` with `-not -path` (filters output, still DESCENDS every build tree) on the hot `just qemu build` path |
| F2 | HIGH | `scripts/build/fixtures-build.sh:258` | fully unpruned `find … package.xml` per fixture build; would also pick up a `package.xml` inside `build-*/` (spurious `nros sync`); `2>/dev/null` hides read errors |
| F3 | MED-HIGH | `scripts/check-no-direct-kernel-alloc.sh:70` | `grep -r` whose `--exclude-dir` misses `target-*`/`build-*`/`_deps`/vendored trees — reads then discards; runs in `check-fast` |
| F4 | MED | `scripts/build/workspace-fixture-signature.sh:39` | signature walk prune omits `_deps`/`install`/`log` → slow + FALSE STALENESS (rebuilt `_deps` changes the hash with no source change) |
| F5 | MED | `scripts/check-fixtures-stale.sh:2,15,31,85` | `set -e` only (no `-uo pipefail`); probe stderr `2>/dev/null` — a CRASHED staleness probe is indistinguishable from a fresh fixture |
| F6 | LOW | `justfile:2163` | clean-bindings prune omits `cargo-target`/`install`/`log` |
| F7 | MED | ≥8 sites | the prune ladder is copy-pasted with divergent subsets — root cause of F1/F3/F4/F6 |
| F8 | LOW | `check-fixtures-stale.sh:59` vs `justfile` test-all | toolchain-presence gate duplicated with different predicates (issue-0030 lockstep pair, unconnected) |
| F9 | LOW | `scripts/check-example-matrix.sh:50` | payload probe filters instead of prunes (bounded by `-quit`, minor) |

## Work items

### W1 — hot-path walks → git index (F1, F2, F3)
- [x] W1.1 (2026-07-24) `qemu-baremetal.just` build+clean: `git ls-files
  'examples/qemu-arm-baremetal/**/Cargo.toml' | awk -F/ 'NF>=4'` (same
  set, zero traversal); clean derives dirs from the tracked list.
- [x] W1.2 (2026-07-24) `fixtures-build.sh` node-pkg presync: `git ls-files
  "examples/$platform/**/package.xml"`; drop the `2>/dev/null`.
- [x] W1.3 (2026-07-24) `check-no-direct-kernel-alloc.sh`: tracked-file enumeration
  (mirror the converted `check-weak-symbols.sh:36` shape).

### W2 — signature + probe integrity (F4, F5)
- [x] W2.1 (2026-07-24) `workspace-fixture-signature.sh`: enumerate via
  `git ls-files -- "$workspace"` (tracked-only; keeps the extension
  allowlist + content-hash + `sort -z` determinism). Closes the
  false-staleness class structurally, not by patching the prune list.
  NOTE: every workspace fixture re-reads STALE once after this lands
  (signature format change) — one full fixture rebuild follows the merge.
- [x] W2.2 (2026-07-24) `check-fixtures-stale.sh`: `set -euo pipefail`; probe stderr
  captured (not discarded); `parallel --halt now,fail=1` so a probe crash
  fails loudly instead of reading as "fresh".

### W3 — unify the prune-ladder SSoT (F6, F7, F9)
- [x] W3.1 (2026-07-24) `scripts/build/prune-dirs.sh`: ONE canonical definition
  (shell array + a `find`-prune-args builder + a grep-exclude builder).
  Sourced by every remaining legitimate walker (clean recipes, matrix
  payload probe, any future one).
- [x] W3.2 (2026-07-24) Convert `justfile:2163` clean-bindings + `check-example-matrix.sh`
  payload probe to the shared definition; sweep `just/*.just` +
  `scripts/` for any remaining inline ladder (grep `-not -path` /
  `--exclude-dir` / `-name 'target*' -prune`) and convert or annotate WHY
  a walk is required.

### W4 — toolchain-gate dedupe (F8)
- [x] W4.1 (2026-07-24) `scripts/test/toolchain-gate.sh` sourced by both
  `check-fixtures-stale.sh` and the `test-all` env_exclude block (the
  issue-0030 lockstep pair becomes one definition).

## Acceptance
- `just qemu build` enumeration + `check-fast` complete with zero
  descent into `target*`/`build*` trees (verify: `strace -e openat` spot
  check or timing on a cold cache).
- An intentionally crashed staleness probe FAILS `check-fixtures-stale.sh`
  (not silently "fresh").
- `git grep -nE "(-not -path|--exclude-dir=)" just/ scripts/` finds only
  sites reading the shared prune definition (or carrying a WHY comment).
- `just ci` green after one post-W2 fixture rebuild.

## Out of scope
- fixtures.toml schema changes (the `signature-inputs` manifest verb is
  covered by W2.1's `git ls-files` route without schema work).
- The pre-existing `nros-rmw-zenoh-staticlib` host panic-handler red.
