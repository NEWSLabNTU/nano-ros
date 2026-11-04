---
id: 2254
title: "An interrupted `nros build` leaves its jobserver FIFO directory in the temp dir"
status: open
type: bug
severity: low
area: [cli, build]
related: [issue-2239, rfc-0117, phase-489]
found: 2026-11-05
---

## What

`nros build` makes a fifo-form jobserver (RFC-0117 D4) in a private
directory, `std::env::temp_dir()/nros-jobserver-<pid>-<seq>/fifo`
(`packages/cli/nros-cli-core/src/builder/jobserver.rs`, `create_fifo`). The
directory is removed only by `impl Drop for Jobserver`. A signal that ends the
process (Ctrl-C, SIGTERM, SIGKILL) skips `Drop`, so the directory and its FIFO
stay behind.

Measured on 2026-11-05 while testing issue 2239: one Ctrl-C and one SIGKILL of
`check-template-copy-out` left `nros-jobserver-625846-0` and
`nros-jobserver-644324-0` in `$TMPDIR`. Passing runs left none.

Each one is a directory holding one FIFO, so the leak is small; but it grows
by one entry per interrupted build and never shrinks. That is the same
"nothing retires it" class issue 2239 fixed for the gates' scratch trees.

## Why it cannot just be unlinked early

A fifo-form client finds the pool by PATH (`--jobserver-auth=fifo:<path>`), so
the FIFO must exist for as long as any `make` child can open it. Unlinking it
right after `open` would break the clients the jobserver exists for.

## What would close it

Candidates, to choose by measurement:

- When creating a pool, retire any `nros-jobserver-<pid>-*` directory whose
  `<pid>` is not a live process (the orphan rule
  `scripts/build/gate-scratch.sh` applies, with the same pid-plus-start-time
  check so a recycled pid is not mistaken for the owner).
- Put the directory under the build's own `build/` instead of the temp dir,
  where the build tree's own lifecycle owns it.
- Remove it from a SIGINT/SIGTERM handler as well as from `Drop` (this does not
  cover SIGKILL, so it would still need one of the above).

Acceptance: interrupting `nros build` with Ctrl-C, SIGTERM and SIGKILL, then
running it again, leaves no `nros-jobserver-*` directory whose owner is gone.
