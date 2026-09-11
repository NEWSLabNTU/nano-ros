---
id: 1311
title: "`check::build`'s `rmw-cyclonedds` lane: idlc-generated `SumSeq.c` fails to
  compile and `service_roundtrip` cannot link — seen twice, NOT yet reproduced on
  a main checkout"
status: open
type: bug
area: rmw, cyclonedds, build
severity: medium
related: [issue-0319]
---

## Read this first

This was seen in two FRESHLY PROVISIONED agent worktrees on 2026-09-11. It was
never reproduced in a long-lived checkout of `main`. CLAUDE.md's rule on
filing from a failing fixture (issues 0859–0862, all retracted) applies:
**reproduce it on a clean `main` checkout before fixing anything.** If it does
not reproduce, the defect is in how a worktree provisions cyclonedds. That is
still worth fixing, but it is a different issue, and this one should be
retitled.

## What was seen

`just ci gate` → `check::build` → the `rmw-cyclonedds` gate (the Cyclone C/C++
test suite, `packages/rmw/cyclonedds/nros-rmw-cyclonedds/tests/`). It failed the
same way twice, each time on a branch whose diff touched nothing under
`packages/rmw`, `third-party`, msg-to-idl or rosidl:

- **Branch `fix/1284-backing-meets-default`:**
  `nros_rmw_cyclonedds_service_roundtrip` (tests `CMakeLists.txt`, line ~328)
  failed to link:
  `undefined reference to 'nros_test_srv_dds__SumSeq_Request__desc'` and the
  same for `_Response__desc`.
- **Branch `fix/1285-followup-rtos-substring`:** one step earlier. Cyclone's
  `idlc`-generated
  `…/build/tests/cyclonedds-from-msg/nros_test/gen/SumSeq.c` includes its own
  header, yet `uint32_t`, `NULL` and the `DDS_OP_*` opcodes come out
  undeclared.

The two are consistent with one cause. If `SumSeq.c` does not compile, its
`*_desc` symbols never exist, and the roundtrip test cannot link.

In both worktrees `third-party/dds/cyclonedds` had just been initialised with
`git submodule update --init` (non-recursive, at its recorded pin), because
agent worktrees start without it.

## Hypotheses, most likely first

1. **Worktree provisioning.** The `idlc` that ran is not the one built from
   this checkout's cyclonedds, or the generated header directory is not on the
   include path the first time round. Check:
   `ninja -C <build> -t query <SumSeq.c.o>` and the `idlc` path in `build.ninja`.
2. **An include the generated TU relies on transitively** (`<stdint.h>`,
   `<stddef.h>`, `dds/ddsi/ddsi_serdata.h`), which a recent header change
   stopped providing. That would reproduce on main.
3. **The msg-to-IDL step emitting a `SumSeq.idl`** whose generated C sits in a
   directory shadowed by another `SumSeq.h`.

## Where it runs

`rmw-cyclonedds` is a `check::build` gate, so it runs in `just ci gate` locally.
Per CLAUDE.md the Cyclone suite belongs in a `check-*` lane (issue 0319); check
which CI events reach it before judging how long it could have been red
unnoticed.

## Acceptance

Reproduced on a clean `main` checkout or ruled out, with the evidence in this
issue. If it reproduces, the fix lands with the gate green. If it doesn't, the
provisioning defect is filed separately and this issue is closed as not a
defect on main.
