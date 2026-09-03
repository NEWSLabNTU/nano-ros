---
id: 1021
title: "zenoh-pico 1.8.0 does not compile with `Z_FEATURE_MATCHING=0`: an
  unguarded call to a MATCHING-only function"
status: open
type: bug
area: rmw, third-party
related: [phase-415, issue-0910]
---

## What

Upstream zenoh-pico `1.8.0` fails to compile when `Z_FEATURE_MATCHING=0`:

```
src/net/filtering.c:330:5: error: implicit declaration of function
  '_z_write_filter_ctx_remove_callbacks';
  did you mean '_z_write_filter_ctx_remove_local_match'?
```

`_z_write_filter_clear` is unguarded, but the function it calls is declared
(`include/zenoh-pico/net/filtering.h:95`) and defined (`src/net/filtering.c:342`)
inside `#if Z_FEATURE_MATCHING`.

**This is upstream's, not ours.** Pristine `1.8.0` has the same unguarded call
at `filtering.c:323`, and the one commit on our patch line that touches this
file (`98ab67c4 fix: open filters for single-threaded clients`) adds no call
to it.

## Why it matters here

`Z_FEATURE_MATCHING=0` is exactly what nano-ros passes on Zephyr — visible in
every Zephyr build line as `-DZ_FEATURE_MATCHING=0`. So every Zephyr zenoh
image fails to build `libnros` against a pristine 1.8.0. It surfaced the moment
phase-415 moved the patch line to 1.8.0.

## Fix carried on the patch line

`0343ad1b` on `nano-ros` guards the call:

```c
#if Z_FEATURE_MATCHING == 1
    _z_write_filter_ctx_remove_callbacks(_Z_RC_IN_VAL(&filter->ctx));
#endif
```

The guard is right rather than merely expedient: the state it clears,
`_z_write_filter_ctx_t::callbacks` (`net/filtering.h:58`), exists only under
`Z_FEATURE_MATCHING == 1`. With the feature off there is nothing to remove, so
skipping the call leaks nothing.

## Open — what is left to study

1. **Report upstream.** Any embedder turning matching off hits this, so it
   belongs in eclipse-zenoh/zenoh-pico, not only on our fork. Per CLAUDE.md the
   upstream contribution is a SEPARATE line with no 1:1 correspondence — the
   shape there may differ, and our branch does not wait on it.
2. **Is the guard the fix upstream would take, or a symptom of something
   wider?** `_z_write_filter_clear` is one call site; nobody has checked whether
   other MATCHING-only symbols are reachable from unguarded code. A sweep is
   the actual question, not this one line:

   ```bash
   # symbols declared only under Z_FEATURE_MATCHING
   awk '/#if Z_FEATURE_MATCHING/,/#endif/' include/zenoh-pico/net/filtering.h \
     | grep -oP '^\w[\w ]*\**\K_z_\w+(?=\()'
   # then grep each for call sites outside a MATCHING guard
   ```
3. **Which other feature combinations are untested upstream?** MATCHING is one
   axis; nano-ros also builds with `Z_FEATURE_SCOUTING=0`, `Z_FEATURE_RAWETH_
   TRANSPORT=0`, `Z_FEATURE_LINK_UDP_*=0`. If 1.8.0 shipped with a broken
   MATCHING=0 build, the others are worth compiling before trusting them.

## How it was found

`just zephyr build-rust-examples` during phase-415's W3, against the ported
line. Native builds do NOT catch it — the default native configuration has
`Z_FEATURE_MATCHING=1`, so the guarded definition is present and the unguarded
call resolves. Only the embedded feature set reaches it.
