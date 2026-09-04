---
id: 1060
title: "The Corrosion fallback fetches `GIT_TAG v0.6.1` — a ref on someone else's server, so upstream can change which tree we build with no local diff"
status: resolved
type: bug
area: build, tooling
severity: medium
found: 2026-09-04
related: [0500, 0726, phase-420, phase-365]
---

# The one fetch in the tree is the one that is not pinned

`cmake/NanoRosCorrosion.cmake:644` declares:

```cmake
FetchContent_Declare(Corrosion
    GIT_REPOSITORY …
    GIT_TAG        ${_nros_corrosion_tag}
)
```

`_nros_corrosion_pin()` resolves that variable from `[tool.corrosion] upstream`
in `nros-sdk-index.toml`, which today holds the **tag** `v0.6.1`.

A tag is a ref on a server we do not control. If upstream retags, this build
switches to a different tree and **no file here changes** — no diff, no review,
no gate. That is strictly worse than the submodule rewind
`check-submodule-pins` exists for: there, the pin is a full commit id and the
gate can go and ask the submodule, which is why a backwards move is detectable
even though `-Subproject commit <hex>` is two hex strings no reviewer can order
by eye.

Found by `check-vendor-fetch-pinned` (phase-420 W8), which holds it in a
shrink-only baseline rather than going red on day one.

## Scope, honestly

This is the **fallback** path. The supported one is the SDK store (`nros setup
--tool corrosion`), whose `dist` assets are sha256-verified, and the fetch is
reached only when the store misses. Issue 0500 is the reason that distinction
matters and the reason it is hard to see: the store ACCUMULATES, `find_package`
takes the first prefix that resolves, and **the configure's printed
`nano-ros: Corrosion <ver> via <origin>` line is the only evidence of which one
ran**. So "we mostly use the store" is not a claim anyone can check after the
fact.

## Fix

Two things move together, or the change is half-applied:

1. `nros-sdk-index.toml` gains the release's commit id beside the tag —
   the tag stays, because it is what a human reads;
2. `_nros_corrosion_pin()` returns the commit id, so `GIT_TAG` is a digest.

Then delete the baseline entry (it is inline in
`scripts/check-vendor-fetch-pinned.py`, not a sidecar file); the gate refuses a
baseline row whose fetch is already pinned, so the cleanup cannot be forgotten.

## Related, and larger

`FETCHCONTENT_BASE_DIR` is set **nowhere** in this tree (`git grep FETCHCONTENT`
returns exactly one hit: RFC-0087 proposing it). With the default
`<build>/_deps`, every build directory fetches independently — issue 0500
measured **159** build dirs each carrying their own resolved Corrosion, 139 on
0.5.1 and 20 on 0.6.1. RFC-0087 D5's "a shared cache makes that once per host"
is therefore a statement about a configuration nobody has set, and setting one
is a prerequisite of the first vendor package rather than a follow-up to it.

## Resolution — 2026-09-04

Both halves landed, plus the prerequisite this issue named as "larger".

`_nros_corrosion_pin()` returns the literal commit
`1499b14e4906a2890f5cee1547c8848db261753d` and `GIT_TAG` is
`${_nros_corrosion_commit}`. The tag stays beside it as the human-readable
name, and a `[tool.corrosion]` entry naming a DIFFERENT tag than that commit is
a configure `FATAL_ERROR`, so the two cannot drift apart silently. The commit
was resolved with `git ls-remote` and then CONFIRMED to be a commit rather than
assumed: `v0.6.1` is a lightweight tag, so the ref is the commit itself and
there is no `^{}` line to peel — had it been annotated, taking the bare line
would have pinned a tag object.

The `FETCHCONTENT_BASE_DIR` half was NOT done as written. That variable moves
all three of a dependency's directories, and the shared subbuild dir records
the generator that populated it, so a second build tree on another generator
gets `CMake step for <dep> failed` — a hard error, not a slow path. What ships
instead shares `SOURCE_DIR` + `SUBBUILD_DIR` (together: the clone step keys on
a stamp in the subbuild and `rm -rf`s the source first, so a per-build subbuild
would destroy a shared source on every new build dir) and keeps `BINARY_DIR`
local, at `$NROS_HOME/fetch`.

The baseline entry in `scripts/check-vendor-fetch-pinned.py` REMAINS, and its
reason now says why: `GIT_TAG` is a variable, which the gate cannot follow, so
it cannot ESTABLISH the digest even though the pin is one. That is a limit of
the gate, not an unpinned fetch. Retiring the entry means putting the commit id
literally in the declaration, which today would cost the cross-check above.
