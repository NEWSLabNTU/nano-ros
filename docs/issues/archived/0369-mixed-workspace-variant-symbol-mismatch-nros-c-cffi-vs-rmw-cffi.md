---
id: 369
title: "Mixed C+C++ workspace fixture link-fails: the C msg archive references a `rmw_cffi_rmw_zenoh` config-variant symbol that nros-c (built with `cffi-zenoh-cffi`) never emits"
status: resolved
resolved_in: "phase-327 session (size-derived variant anchor + weak def)"
type: bug
severity: medium
area: build
related: [issue-0360, issue-0268, phase-321]
---

## Finding (2026-08-01, building native workspace fixtures for ci-matrix)

`just native build-workspace-fixtures` link-fails on the **mixed** workspace:

```
/usr/bin/ld: src/c_talker_pkg/libstd_msgs__nano_ros_c.a(std_msgs_msg_int32.c.o):
  (.data.rel.ro+0x0): undefined reference to
  `nros_config_variant_alloc_cffi_zenoh_cffi_platform_posix_rmw_cffi_rmw_zenoh_ros_humble_std'
collect2: error: ld returned 1 exit status
```

## Root cause

The generated C message archive (`libstd_msgs__nano_ros_c.a`) references a
config-variant alloc symbol whose feature suffix includes **`rmw_cffi_rmw_zenoh`**,
but the `nros-c` staticlib linked into the same mixed workspace is built with a
DIFFERENT feature set and so emits a variant symbol WITHOUT that fragment.

From the build log, the mixed workspace's two halves compile with divergent
features:

- `nros-c`   → `--features=ros-humble,cffi-zenoh-cffi,std,platform-posix`
- `nros-cpp` → `--features=ros-humble,rmw-zenoh-cffi,std,platform-posix`

and the feature graphs differ in the RMW spelling:

- `nros-c`:  `cffi-zenoh-cffi = ["rmw-zenoh"]`  (no `rmw-cffi`)
- `nros-cpp`: `rmw-zenoh-cffi = ["rmw-cffi", "nros-c/rmw-zenoh", "dep:nros-rmw-zenoh"]`

So the C-side msg codegen stamps its variant reference from the WORKSPACE feature
set (which carries `rmw-cffi` via the C++ half), producing
`…_cffi_zenoh_cffi_…_rmw_cffi_rmw_zenoh_…`, while the `nros-c` staticlib actually
built for that workspace carries only `cffi-zenoh-cffi` (→ `rmw-zenoh`, NOT
`rmw-cffi`) and emits `…_cffi_zenoh_cffi_…` without the `rmw_cffi_rmw_zenoh`
fragment. The reference is unresolved at link.

This is the same variant-consistency class that issue 0360 ("stamp the feature
variant into the generated headers and archives") addresses, but 0360 does not
reconcile the MIXED workspace, where the C and C++ halves legitimately select
different rmw-feature spellings (`cffi-zenoh-cffi` vs `rmw-zenoh-cffi`) yet must
agree on ONE variant symbol for the shared C msg archive to link.

## Fix direction

The C msg codegen's variant string and the `nros-c` staticlib's emitted variant
must be computed from the SAME feature set for a given workspace build. Either:
1. Build `nros-c` in the mixed workspace with the same effective RMW features the
   C++ half pulls in (so it emits the `rmw_cffi_rmw_zenoh` variant the C archive
   references), or
2. Compute the C-side variant reference from `nros-c`'s OWN features (what it was
   actually built with here), not the union that includes the C++ half's
   `rmw-cffi`.

(2) is the safer contract — the reference should match what the linked `nros-c`
provides, not what the workspace as a whole enables. The right home is the same
codegen/stamp seam issue 0360 touched.

## Scope

Surfaced on the `mixed` workspace (fixtures `workspace-mixed-*`). Pure-C and
pure-C++ workspaces do not trip it because their two halves share one RMW feature
spelling. It blocks `just native build-workspace-fixtures` (the mixed rows), hence
the `native,mixed,*` tier-2 coordinate.

## Repro

```
source ./activate.sh && source /opt/ros/humble/setup.bash
just native build-workspace-fixtures
# … undefined reference to nros_config_variant_alloc_..._rmw_cffi_rmw_zenoh_ros_humble_std
```

## Deepened root cause (2026-08-01, investigation)

The simple fix ("compute the C-side variant from nros-c's own features") is
ALREADY present: nros-cpp's gap-fill copy of the shared C header carries
`/* Issue 0360 — deliberately NO variant stamp here */` (`nros-build-helpers/src/cpp.rs`,
`c_format_header`) precisely so it does not mis-name the symbol. So the union-slug
anchor the mixed build referenced (`alloc`+`rmw_cffi`, which nros-cpp adds via
`rmw-zenoh-cffi = ["rmw-cffi", "nros-c/rmw-zenoh", …]`) did NOT come from
nros-cpp's gap-fill.

It comes from a **second nros-c build**. In a mixed workspace, nros-c is pulled
twice with different features:
- the C msg/staticlib path builds it narrow (`cffi-zenoh-cffi` → no `alloc`/`rmw-cffi`);
- the C++ path pulls `nros-c` through nros-cpp's `rmw-zenoh-cffi` (which unifies in
  `rmw-cffi` + `alloc`).

The shared `nros_config_generated.h` (written by `write_header_if_absent_or_verify`,
"nros-c OWNS this file") gets its `nros_config_variant_<slug>` anchor from whichever
nros-c build writes it FIRST. If the union-featured build writes first, the anchor
is the union slug; the narrow build emits `nros_config_variant_<narrow-slug>`; the
C msg archive (including the shared header) references the union symbol nobody
emitted → undefined reference. `defines_of` passes throughout because the sizes
agree (the whole point) — only the link symbol diverges.

**Why this is not a safe blind fix:** the correct resolution is a design choice in
the freshly-landed #360 variant-stamp — e.g. derive the anchor slug from a
CANONICAL size-determining config subset (identical across both nros-c builds), or
ensure a single nros-c feature set per workspace, or drop the C link-time anchor
and rely on `defines_of` (which already provides the size-safety and tolerates
feature diffs). Each touches load-bearing, actively-worked code and cannot be
verified without a full mixed-workspace rebuild. Left for the #360 owner with this
precise diagnosis rather than risking a regression to the size-safety mechanism.

**Also:** first observed on a pre-rebase tree; several later variant/feature
commits (#360 and follow-ups) landed the same day — reproduce on current `main`
before fixing, in case a subsequent commit already covers it.


## Resolution (2026-08-01)

Fixed on the seam the deepened root cause identified, taking the issue's own
"canonical size-determining subset" direction:

- `variant_suffix_from_sizes()` — the anchor suffix hashes the 21 size VALUES
  the exact header ships (`sz_<fnv64>`), not the cargo feature spelling. The
  two nros-c builds of a mixed workspace (same sizes, different rmw-feature
  spellings) agree on the symbol by construction; a consumer holding a header
  with DIFFERENT sizes still fails to link — the anchor's actual guarantee
  (the 0088…0268 family). The wrong-backend case the feature slug also caught
  (phase-325 W3) still fails on its own missing backend symbols.
- The archive-side definition moved from a strong Rust `#[no_mangle]` static
  to a WEAK C definition (cc-compiled in build.rs): with agreeing suffixes,
  a mixed image links BOTH nros-c builds' objects, and N identical weak defs
  merge where two strong ones would collide.
- The human-readable `NROS_CONFIG_VARIANT` string keeps the feature slug.
- `nros_cpp_config_variant_*` is untouched: one writer + one archive per
  workspace, no divergence to reconcile.

Verified: `just native build-workspace-fixtures` — the exact failing command —
links every workspace including both mixed per-host entries (rc=0, zero
undefined/multiple-definition). The threadx-linux and freertos families hit
the same symbol and re-verify in the fixture campaign.
