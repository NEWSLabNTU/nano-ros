---
id: 411
title: The threadx-linux C workspace entry is resolved under a name the build
  never produces, so its e2e case silently skips
status: resolved  # fixed 2026-08-04
type: bug
area: testing
related: [phase-331, phase-337, rfc-0051]
---

## Problem

The binary resolver asks for an entry that does not exist:

```rust
// packages/testing/nros-tests/src/fixtures/binaries/mod.rs:1846
build_workspace_cmake_entry_in(
    "workspace-c-threadx-linux",
    "c",
    "build-workspace-fixtures-threadx",
    "native_threadx_entry",          // <- the build produces `threadx_entry`
)
```

The manifest row it names says otherwise:

```toml
# examples/fixtures.toml:533
id     = "workspace-c-threadx-linux"
entry  = "threadx_entry"
```

So `entry_e2e::case_01_threadx_linux_c` reports `[SKIPPED]` under
`just test-all` — and a bare `cargo nextest` run counts the same
`nros_tests::skip!` panic as a FAILURE, which is how it was noticed.

## Why it stayed invisible

A missing fixture and an absent toolchain present identically to the resolver:
both are "no binary here". The light tier skips on both, so a coordinate that
never runs looks the same as one that cannot run on this host. That is the
0350 class — a lane staying green because the thing it should have run was
never located.

The entry name is not otherwise wrong: the workspace really does build
`src/threadx_entry/threadx_entry`. Only this resolver disagrees, which points
at a rename that swept the workspace and the manifest but not the one call site
that hardcodes the binary name. `native_threadx_entry` reads like the
pre-consolidation spelling, so phase-331 is the likely origin.

## Direction

Fix the string, then ask the 0196 question: is this the only site? The resolver
hardcodes entry binary names for every workspace family, so the same rename
could have missed siblings. A gate is available almost free — every
`build_workspace_cmake_entry_in` call names a manifest `id`, so the entry
argument can be checked against that row's `entry =` field instead of being
written twice. Two spellings of one fact is what produced this.

Found by phase-337 W4 while thinning the ThreadX boards. Not that wave's to
fix: it predates it and belongs to whichever phase owns the rename.

## Resolution (2026-08-04)

Resolver now names `threadx_entry`, matching the manifest and the workspace.

Fixed as a CLASS, not a site. `build_workspace_cmake_entry_in` takes both the
fixture id and the entry name, and the manifest already says which entry that id
builds — so the two can disagree, and did, silently. The helper now looks the
entry up (field 5 of the manifest record it already fetches) and FAILS LOUDLY on
a mismatch instead of resolving a path that will never exist. That covers ~20
call sites, any of which could drift the same way; a fix at the one site that
happened to drift would leave the rest exposed.

Swept the other callers first: no further mismatches, so this was the only one.

Verified both directions — with `native_threadx_entry` restored the helper
reports "resolves entry … but examples/fixtures.toml declares …", and with the
correct name `entry_e2e::entry_matrix` passes (the cell resolves instead of
reporting "not built").
