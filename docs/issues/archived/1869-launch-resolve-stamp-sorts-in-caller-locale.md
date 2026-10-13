---
id: 1869
title: "The launch resolver's content stamp sorted its file list in the caller's locale, so one tree read fresh under C.UTF-8 and stale under en_US.UTF-8"
status: resolved
type: bug
area: [build]
severity: low
found: 2026-10-13
resolved: 2026-10-13
related: [0596, 0363, 1487]
---

## Measured

A downstream doctor check (simple-autoware-safety-island, U4) runs
`scripts/check-launch-resolve-fresh.sh` against this repo's resolver. On one
unchanged tree it reported the resolver STALE from an `en_US.UTF-8` shell and
current from a `C.UTF-8` one; the stamp beside the binary had been written by a
`just setup-launch-resolve` run under `C.UTF-8`. The island works around it by
accepting a match under the caller's locale or C.

## Cause

`nros_launch_resolve_stamp` (`scripts/build/launch-resolve-stale.sh`) pipes the
resolver's `.rs` / `Cargo.toml` / `Cargo.lock` list through a bare `sort` and
hashes the lines in that order. `sort` collates in the inherited locale, and
en_US.UTF-8 ignores `_` and `.` at the first collation level while byte order
does not, so `a_c.rs` / `ab.rs`-shaped neighbours swap. Same contents, two
digests: the writer and the reader disagree whenever their locales differ, and
`setup-launch-resolve` rebuilds an unchanged resolver after every locale switch.

## Fix

- The stamp sorts with `LC_ALL=C sort` (byte order), so the digest depends on
  the tree only. A stamp written under a collating locale before this change
  reads stale once and is rewritten by the next `setup-launch-resolve`.
- `tests/launch-resolve-stamp-locale-tests.sh` (`just check
  launch-resolve-stamp-locale`): one fixture tree stamped under C, C.UTF-8 and
  en_US.UTF-8 must agree; an edited source must still move the stamp; negative
  control: a bare `sort` over the same names must disagree between the two
  locales (NOT VERIFIED via `nros_check_unverified` on a host without
  en_US.UTF-8).

## Evidence

- New test against the fixed script: `[PASS] ... 3 case(s) held`.
- Same test with the bare `sort` restored: `[FAIL] launch-resolve-stamp-locale:
  stamp differs between C (c0aa848b...) and en_US.UTF-8 (e0225ca3...)`.
