---
id: 2608
title: "A FAILED metadata probe build is reported as \"no producer\" and `nros configure` ends `sync: done`, exit 0"
status: open
type: bug
area: [cli, metadata, testing]
severity: medium
found: 2026-11-15
related: [phase-559, 0445, 0257, 1469]
---

**Owner:** nros 175. **Home:** phase-559 W11. Found while running phase-559
W10's smoke test; it is not fixed inside W10.

## What happens

`nros dev bindings`, and every `nros configure` that refreshes source metadata,
folds two different facts into one list, `RefreshReport::unsupported`
(`orchestration/metadata_refresh.rs`):

1. **A component nothing can probe:** its language has no producer, or it
   cannot be probed now. That is the fallback this list was made for (phase-307
   W3), and it is reported so the bake knows the entity count is unknown (issue
   0257).
2. **A component whose probe BUILD FAILED.** Its `Err(why)` is pushed onto the
   same list with the build error in parentheses.

`cmd/ws.rs` prints every entry as `sync: source metadata — no producer for
<pkg>::<comp> (<why>)`, then prints `sync: done.` and returns `Ok(())`.
Measured 2026-11-15 on `examples/workspaces/c`, in a worktree whose zenoh-pico
submodule was not checked out:

```
sync: source metadata — no producer for talker_pkg::talker (metadata probe build failed for `talker_pkg::talker` (exit 2):
  ... error: failed to run custom build command for `zpico-sys v0.6.0 ...`
  zenoh-pico source not provisioned at ".../zpico-sys/zenoh-pico". Run: nros setup --source zenoh-pico ...
sync: done.
```

Every probe in the workspace failed, and the command exited 0. A second run
failed every probe for a different reason (W10's shared build-dir check
refused it) and read exactly the same way.

## Why it matters

This is issue 0445's shape: a broken build is replaced by a message that
explains itself, and the caller sees green. A script or CI lane that runs
`nros configure` and checks its exit status learns nothing. The workspace then
bakes with every sidecar missing, which issue 0257 is about. The error text
is there, but it is worded as "no producer", which tells the reader that
nothing is wrong.

## Expected

- The two facts are reported apart. A failed probe is a FAILURE, named by
  node, with the first error line and where the full log is.
- A failed probe makes the command exit non-zero. At minimum, a run where
  EVERY probe failed must refuse rather than print `sync: done`.
- The real no-producer fallback (case 1) stays as it is: reported, not fatal.
- Issue 1469's "probe failed at this source last sync; unchanged" entry is a
  recorded failure too, so it follows the same rule.

## Where to look first

`metadata_refresh.rs`: the `Err(why) => report.unsupported.push(..)` arm in the
C/C++ batch, and the other `unsupported.push` sites that carry a probe error.
They need a `failed` list beside `unsupported`. `ws.rs`'s report block is where
the exit status is decided. Check who calls `refresh_stale_sidecars` and reads
`unsupported` (regenerate-bindings, fixture builds) before changing the exit
code: some may rely on sync never failing on a probe.
