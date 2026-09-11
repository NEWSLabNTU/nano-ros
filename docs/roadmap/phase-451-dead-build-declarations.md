# Phase 451 — a dead build declaration that reads as authoritative

**Status (2026-09-11). Opened to give three homeless issues one owner. Nothing
in this phase has landed; W1–W3 are open.**

## Why this phase exists

Three open issues report a build declaration that no longer does anything and
that a reader cannot tell is dead. They are small, and the reason to give them
one owner rather than three is that their cost is measured in reader time, which
is invisible per-site and adds up:

* [#1218](../issues/1218-dead-nanoroslink-duplicate.md) —
  `packages/api/nros-c/cmake/NanoRosLink.cmake` defines the public verb
  `nano_ros_link_rmw` and nothing includes it. It **misled two of four
  independent readers in one session**, because it holds a fourth closed RMW
  list and a force-link that does not happen. It was already finding A1 of a
  codebase audit and had no tracked owner.
* [#1213](../issues/1213-zpico-net-size-probe-publishes-to-a-retired-crate.md) —
  `probe_net_type_sizes` publishes `sizeof(_z_sys_net_socket_t)` /
  `sizeof(_z_sys_net_endpoint_t)` through three carriers, two of them aimed at
  `zpico-platform-shim`, a crate phase-129.D deleted. Its doc comment states the
  opposite of what the code does.
* [#1217](../issues/1217-workspace-exclude-list-is-unaudited.md) — the root
  `Cargo.toml` carries 57 `members` and **174 `exclude` entries, 36 of which
  name directories that do not exist**, and one host-buildable crate
  (`packages/rmw/transport-callbacks`) is excluded for no discoverable reason.

The pattern is one thing: **a declaration whose only remaining effect is on
belief.** A dead `exclude` line, a dead cmake module and a dead `cargo:` carrier
all compile, all pass every gate, and all answer a reader's question wrongly.

## What makes this worth a phase rather than three commits

Deleting each is a few minutes. Knowing each is dead is not — every one of these
was established by a whole-tree grep with exclusions, and #1218's cost was paid
four times over before anyone ran one. So each work item owes the same two
things: the evidence that the declaration is dead, and a way for the NEXT dead
one to be found without re-deriving it.

The exclude list is where that generalises: 36 of 174 entries naming absent
directories is not three mistakes, it is an unmaintained list, and a list nobody
maintains grows the fourth closed RMW list in #1218 all over again.

## Work items

### W1 — the dead `NanoRosLink.cmake` duplicate goes

[Issue 1218](../issues/1218-dead-nanoroslink-duplicate.md). The live copy is
`cmake/NanoRosLink.cmake`, included by five platform modules as
`../NanoRosLink.cmake`. The `packages/api/nros-c/cmake/` copy is included by
nothing and is not installed.

- [ ] The dead copy is deleted, with the grep that establishes it in the commit
      message.
- [ ] The closed RMW list it carried is checked against the live one first — a
      fourth copy of a list is a fact about the list, and if the live one is
      missing something the dead one had, that is a finding, not debris.

### W2 — the zpico size probe publishes only to a reader

[Issue 1213](../issues/1213-zpico-net-size-probe-publishes-to-a-retired-crate.md).
Two of three carriers target a deleted crate; the doc comment describes the dead
path as the live one.

- [ ] The dead carriers are removed and the comment describes what remains.
- [ ] The surviving carrier's reader is named in the comment, so the next
      deletion of that reader makes this dead loudly.

### W3 — the root `exclude` list is audited and kept honest

[Issue 1217](../issues/1217-workspace-exclude-list-is-unaudited.md). Every
legitimate exclusion in this tree satisfies one of five structural reasons — own
`[workspace]` table, own tracked `Cargo.lock`, a `.cargo/config.toml` pinning a
non-host `[build] target`, a cross-only dependency set, or "metadata only, no
Rust targets". Two entries fail that audit and 36 name nothing at all.

- [ ] The 36 absent entries are removed.
- [ ] `packages/rmw/transport-callbacks` is either given a reason or made a
      member — the issue records that it builds on the host.
- [ ] A gate: an `exclude` entry must name an existing directory AND satisfy one
      of the five reasons, the reason being derivable rather than a comment.
      Without this the list is unmaintained again in a month, which is the whole
      finding.

## Acceptance for the phase

* `grep`-reachable: no cmake module defining a public `nano_ros_*` verb is
  unreachable from any `include()`.
* The root `exclude` list is machine-checked, and the check fails on a
  deliberately added stale entry.

## Non-goals

* A tree-wide dead-code sweep. These three were filed; the periodic audit
  ([docs/development/codebase-audit-checklist.md](../development/codebase-audit-checklist.md))
  owns finding more.
* Anything about what the RMW lists should CONTAIN — that is
  [phase-444](phase-444-rmw-fix-up.md). W1 only checks the dead copy against the
  live one before deleting it.
