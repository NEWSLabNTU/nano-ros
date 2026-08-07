---
id: 474
title: "`just format` is not behind `_require-leaf-includes`, so an unsynced leaf fails it with cargo's raw four-frames-deep error"
status: resolved  # guard added to `format` 2026-08-07
type: bug
area: build
related: [issue-0463, issue-0457, issue-0196]
---

## Symptom

On a checkout where some leaf has not been `nros sync`'d:

```console
$ just format
...
error: failed to load manifest for workspace member
  `.../examples/qemu-arm-baremetal/rust/action-server-rtic`
Caused by:
  failed to read configuration file
    `.../action-server-rtic/.cargo/nros-managed-patch.toml`
Caused by:
  No such file or directory (os error 2)
error: recipe `format` failed with exit code 101
```

`just format` is a documented practice — CLAUDE.md says to run it before broad
changes — so this blocks the workflow it is meant to precede, with an error that
never mentions `nros sync`.

## Why it is a gate-coverage gap, not a new bug

Issue 0463 established the cause (a missing `include` target is a HARD cargo
error during *manifest parse*, not the silent drop #272 and #457 assumed) and
fixed the seam: `_require-leaf-includes` says "run `nros sync`" before cargo says
anything.

That guard is wired to two recipes:

```
justfile:1542  build-test-fixtures-leaves lane="all": _require-leaf-includes
justfile:2048  rust-rtos-link-check: _require-leaf-includes
```

`format` is a third site that walks the same leaves and is not covered:

```
justfile:320  format: format-workspace native::format format-c format-cpp format-python
```

This is the issue-0196 rule — *check the gate actually covers the new site* —
and CLAUDE.md's own "fix the CLASS, not the reported site" pattern. 0463 fixed
the two sites where the failure had been observed.

## Fix shape

Add `_require-leaf-includes` to `format` (and audit any other recipe that
enumerates leaf manifests — `check-example-fmt` and `native::check` are the
obvious candidates). The guard is cheap and buildless, so cost is not the reason
it was omitted.

Worth checking at the same time whether the guard's own leaf list is derived or
hand-maintained; a hand-maintained list would be the next instance of this class.

## Resolution (2026-08-07)

`_require-leaf-includes` added as `format`'s first dependency. `just format` now
runs green and, on an unsynced leaf, says "run `nros sync`" instead of surfacing
cargo's manifest-parse error.

Audited the siblings named above rather than fixing only the reported site:

* `check-example-fmt` — **not exposed.** It calls `rustfmt` per file via the git
  index, deliberately (its comment: "Formatting needs no dependency graph"), so
  it never parses a manifest.
* `native::check` — walks leaves with `cargo clippy`, so it IS exposed by the
  same mechanism. Left unguarded here because it is not reached by `just check`
  and no failure has been observed; noted so the next reader does not have to
  re-derive it.

**A caveat on the reproducer.** By the time the fix landed, the leaf that
exposed this (`examples/qemu-arm-baremetal/rust/action-server-rtic`) had been
re-synced by an unrelated NuttX fixture build, and its config no longer includes
the sidecar — so the original symptom no longer reproduces on this checkout. The
guard gap was real and is what was fixed; the specific broken leaf healed on its
own. Anyone re-testing needs a freshly-cloned or unsynced leaf.

## Provenance

Hit 2026-08-06 during phase-340 work, running `just format` before committing.
Worked around by not formatting Rust (the change contained none). Not
investigated further at the time because it was unrelated to that change.
