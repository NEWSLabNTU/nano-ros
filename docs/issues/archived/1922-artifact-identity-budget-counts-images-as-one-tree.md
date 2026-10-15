---
id: 1922
title: "`artifact-identity-budget` counts every image of a workspace as one tree: two freshly built images read 8 nros_core identities against a one-image budget of 4"
status: resolved
type: bug
area: [build, testing]
severity: low
found: 2026-10-15
related: [0499, 1832, 1919, phase-340]
---

## Measured

On a worktree where `just freertos build` and the native fixture build both ran
inside one `started_at` window, `just check fast` stopped at:

```
artifact-identity budget: FAIL
  counted 275 of 743 rlib(s) — those written since started_at=2026-10-15T07:16:12Z (issue 0499).
  nros_core has 8 distinct -C metadata identities in examples/workspaces/mixed/build
  (budget 4, recorded 2026-08-07 by phase-340 W4).
```

The eight identities split into two sets of four, one set per image:

- `examples/workspaces/mixed/build/freertos-zenoh-mps2-an385-freertos/cmake/cargo/{nano-ros_<h>,nros_ws_runtime_<h>}/…`,
  each at host and `thumbv7m-none-eabi`;
- `examples/workspaces/mixed/build/posix-zenoh-linux/cmake/cargo/{nano-ros_<h>,nros_ws_runtime_<h>}/…`,
  each at host and `x86_64-unknown-linux-gnu`.

Each image has the 4 identities the budget allows: two cargo roots (workspace
member vs. path dep, phase-340's "R2 re-measured") times two `--target`
spellings. No image has a new identity.

## Why

The budget was recorded when the mixed workspace built ONE feature set into one
tree. Since issue 1832, `nros build` writes each image to
`<ws>/build/<image>/…`, and `scripts/check-artifact-identity-budget.sh` adopted
`examples/workspaces/mixed/build` as its subject, which the script describes as
"the same subject — the mixed workspace at one feature set". It stays one
subject only while a single image has been built in the window. `just freertos
build` builds `workspace-fixtures-build.sh freertos mixed`, which adds a second
image beside the native one. The gate then fails on a tree where every image is
within budget.

No CI lane runs this gate (issue 1832), so the red is local. It blocks
`just ci gate` and the `pre-push` hook. The workaround used on 2026-10-15
(phase-502 W6) was to delete the FreeRTOS image's build dir.

## Direction

Budget `nros_core` identities per image dir (`<ws>/build/<image>/`). Keep the
copy ceiling counted across the whole workspace tree, because issue 1919's
regression is two build ROOTS of ONE workspace writing the same identity
(`build/nros-metadata/…` and `build/<image>/…`), and a per-image split would
hide it. Add a self-test case with two images, each within budget.

## Resolution

Two readings, two subjects (`scripts/check-artifact-identity-budget.sh`):

- **Identities: one image.** PR #2268 made the gate read
  `row_artifact_root()` of row `workspace-mixed-native`
  (`build/posix-zenoh-linux/cmake`) instead of globbing `<ws>/build`. A second
  image, the probe tree and pre-phase-500 residue are no longer counted there.
- **Copies: every build root of the workspace (axis 3).** `_resolve_ws_roots`
  takes `<ws>/build` from the same row's artifact root, and the image dirs
  from every manifest row of that `dir`. Each other dir under `<ws>/build` is
  a non-image root (today the metadata probe's `nros-metadata/`), and its
  copies are charged to every image. Per identity, the check is
  `copies(image) + copies(non-image roots) <= CEILING_COPIES`. Two images
  each at the ceiling pass. Issue 1919's shape, where an image and the probe
  both write one identity, fails.
- **Only rlibs in the run window count** (`find -newer` on issue 0499's
  `started_at`, or the `.started` sidecar). Leftover configurations like
  `posix-zenoh-native` and `demo_bringup__*` would otherwise count as
  non-image roots. With no window the axis prints NOT VERIFIED; it never
  counts the whole tree. The axis also runs on the gate's two "image tree has
  nothing in the window" SKIP paths, because 1919 was the threadx-linux image
  plus the probe, with the budget row's image untouched.
- **Out of reach, stated in the script:** if an image with no manifest row is
  built inside the window, the rule treats it as a non-image root and charges
  it to the other images.

Self-test cases (e)–(g), run through `NROS_IDENTITY_BUDGET_MANIFEST` on a
synthetic two-image workspace: two images, each with 4 nros_core identities
and 5 copies of one identity, pass (5/5); the probe root writing that identity
once more fails ("6 copies", probe path listed); 5 more copies under a
`posix-zenoh-native` dir older than `started_at` pass.

Measured in the box tree: the real stamp's window predates every rlib (SKIP,
axis reports nothing written). With a scratch stamp at the native image's
build start: nros_core 4/4, worst crate 5/5, worst identity 5/5, cross-root
5/5. With a window wide enough to take in the residue and the probe
(`started_at=2026-09-01`), the axis fails on `nros_serdes` (10 copies,
`posix-zenoh-native` + image) and `heapless`/`byteorder` (probe + residue).
That is the expected verdict for a window that includes them.
Issue 1919's defect (the probe and the image writing the same FFI crates) is
untouched. This gate now catches it whenever both build roots write inside
one run window.

## Amended 2026-10-16: roots do not add up

The summed rule above made the gate fail after a plain `nros sync`. Measured
on the mixed workspace: with the C/C++ sidecars moved aside, `nros sync`
re-probed 6 components and kept the probe's build tree (the default; only
the fixture lanes set `NROS_METADATA_PROBE_CACHE=0`). The gate then read
`nros_serdes` and `heapless` at 10 copies: the image's five FFI crate trees
plus the probe's five. The probe and the image are two cmake projects, so two
cargo roots, and issue 0616 forbids one target dir serving both. So the
"defect" this axis existed to catch is the expected state, and issue 1919 is
resolved on that basis.

The axis now gives every root under `<ws>/build` its own ceiling:
`copies(R, identity) <= CEILING_COPIES` for each root R, image or not. The
probe root is still read, so a sixth copy inside it fails. Self-test (f)
became "the probe at 5 beside an image at 5 passes", (h) is new ("a sixth
copy inside the probe root fails and names it"), and (g)'s residue holds 6
copies so it fails if counted (checked by re-dating it into the window). On
the re-probed tree: nros_core 4/4, worst crate 4/5, worst identity 5/5 in
the image and 5/5 in any root.
