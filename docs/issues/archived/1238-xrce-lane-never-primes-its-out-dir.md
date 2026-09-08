---
id: 1238
title: "`just check rmw-xrce` could only pass against a cache somebody primed by hand — fresh tree says `NROS_XRCE_CFFI_OUT_DIR is not set`, stale tree names a directory cargo replaced"
status: resolved
type: bug
area: build, rmw
severity: medium
found: 2026-09-09
resolved: 2026-09-09
related: [0787, 0952, 1018]
---

# The lane that says it "does both halves" does neither

phase-420 W9 step 4 stopped `packages/rmw/xrce/nros-rmw-xrce` from compiling
the vendored micro-XRCE-DDS-Client / micro-CDR sources a second time: it now
LINKS the archive the cargo lane builds, so the CTest harness exercises the
objects images actually ship. That is the right shape. Its configure therefore
needs `NROS_XRCE_CFFI_OUT_DIR`, and the CMakeLists says so twice — once as a
`FATAL_ERROR` when the variable is unset, and once when the directory it names
holds no `nros-xrce-vendor-build.txt`. The second message ends:

> Run `cargo build -p nros-rmw-xrce-cffi` — or just `just check rmw-xrce`,
> which does both halves.

**`just check rmw-xrce` did not do either half.** The recipe was:

    BD="$(nros_build_dir "$NROS_KIND_XRCE_CHECK")"
    cmake -S packages/rmw/xrce/nros-rmw-xrce -B "$BD" -DCMAKE_BUILD_TYPE=Release

No cargo build, no `-DNROS_XRCE_CFFI_OUT_DIR`. So the lane had exactly two
outcomes and both are red:

* **fresh build dir** — `CMakeLists.txt:90`, "NROS_XRCE_CFFI_OUT_DIR is not
  set", every time. Measured against `/tmp` with the submodule present.
* **existing build dir** — the value comes from `CMakeCache.txt`, written by
  whatever hand-run last primed it. The OUT_DIR is FINGERPRINT-named, so it
  moves whenever anything in the crate's dependency closure changes; the cached
  path then names a directory cargo has replaced and the configure dies at
  `CMakeLists.txt:106` instead.

The second is what was actually observed: a `nros-node/build.rs` edit
(unrelated, issue 1233) re-fingerprinted the closure, six
`nros-rmw-xrce-cffi-*` hash directories existed, and the cache pointed at a
seventh that no longer did. Rebuilding the crate by hand did not help — that
writes a NEW hash directory and leaves the cache pointing at the old one.

## Why it stayed invisible

The lane lives in `check-build`, which is `schedule` / `workflow_dispatch`
only, so no merge-gating event runs it (this is deliberate — it needs generated
bindings and prebuilt stamps no CI job builds). A uniformly-red lane has no
signal capacity: a regression landing in it looks exactly like yesterday's
failure. Issue 0952's rule, one lane over.

Issue 0787 added this lane precisely because the xrce C ABI seam had no
compiler looking at it on the host. It has had none since W9 changed how the
project links.

## Fixed

`scripts/build/xrce-cffi-out-dir.py` builds the crate and prints the OUT_DIR
its build script ran in, read out of `--message-format=json`'s
`build-script-executed` record (emitted for a FRESH unit too — cargo replays
it, so the lookup does not depend on a rebuild). The lane calls it and passes
the result as `-DNROS_XRCE_CFFI_OUT_DIR`, on EVERY run, so a cache can never
outlive the path it holds.

One spelling, because there were two and neither ran: the by-hand pipeline the
CMakeLists prints in its error, and a lane that passed nothing at all.

Acceptance: `just check rmw-xrce` from a tree whose xrce build dir is stale —
now `2/2 tests passed`, where before it was `Configuring incomplete`.
