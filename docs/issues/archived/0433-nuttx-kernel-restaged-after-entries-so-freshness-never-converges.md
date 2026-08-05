---
id: 433
title: The NuttX kernel is re-staged after the entries link, so the fixture freshness probe can never converge
status: resolved  # root-caused + worked around 2026-08-05
type: bug
area: testing
related: [phase-337, rfc-0069, issue-0418]
---

## Problem

`just nuttx build-fixtures` exits 0, and the fixtures it just built immediately
read STALE:

```
Failed to build first binary (nuttx rust Action): BuildFailed(
  "Test fixture is STALE — a source is newer than the built binary:
     binary: …/rust/action-server-entry/target/armv7a-nuttx-eabihf/nros-minsizerel/nuttx_rs_action_server_entry
     newer:  …/third-party/nuttx/nuttx/staging/libc.a")
```

Measured right after a green build:

```
20:42:48  …/nuttx_rs_action_server_entry
20:46:00  third-party/nuttx/nuttx/staging/libc.a
```

The kernel artifacts (`staging/libc.a`, `include/nuttx/config.h`) are written
**3 minutes after** the entry links against them. They are inputs to the entry,
so the probe is right that the binary is older — but running the build again
reproduces the same ordering. Two consecutive `just nuttx build-fixtures` runs,
both `rc=0`, leave the same four cells unrunnable.

Confirmed not a one-off: `nuttx c Action` fails the same way against
`third-party/nuttx/nuttx/include/nuttx/config.h`.

## Why it matters

`nuttx rust` and `nuttx c` action cells cannot be run at all — not "fail", but
never execute, reported as a skip-shaped failure. That is the 0350 class: a
coordinate that never runs looks the same as one that cannot run on this host.

It blocked RFC-0069's last acceptance item (every action Runtime cell green on
real targets, the raw↔raw pairs the payload-envelope change actually alters).
`nuttx cpp` passes, so the lane is half-verified in a way no summary shows.

## Not the cause

Two other things were wrong on this path and are FIXED, so they will not confuse
the next reader:

* Stale `CMakeCache.txt` files naming `packages/boards/nros-board-nuttx-qemu-arm`
  — the board dir phase-337 W3 consolidated into `nros-board-nuttx-qemu`. Five
  workspace build dirs plus twelve example ones. Wiped.
* `_nros_profile_query args` returning `--profile nros-minsizerel` as ONE string,
  which the nuttx carve-out mapfiled into a single argv element and cargo
  rejected. Fixed in `nros_cargo_profile_args_for`.

With both fixed the nuttx build goes green; this issue is what remains.

## Fix direction

Either stage the kernel BEFORE the entries link (the dependency order the probe
already assumes), or exclude the regenerated kernel artifacts from the entry's
input signature and depend on the kernel's own inputs instead. The first is
probably right — the current order means the linked entry and the staged kernel
are not provably the same build.

## Root cause (2026-08-05) — arm and riscv share ONE kernel tree

The title says "re-staged after the entries link", which is the symptom. The
cause is that `third-party/nuttx/nuttx` is a single configured tree shared by
both architectures, and `build-fixtures` runs `build-fixtures-arm` then
`build-fixtures-riscv`. The riscv half reconfigures that tree and re-stages
`staging/*.a` for rv-virt — after the arm entries have already linked against
the arm staging. One run shows it plainly: two `Building NuttX...` full
rebuilds AND two "export up-to-date — skipping" in the same invocation.

So after a full `build-fixtures`, one architecture's entries are ALWAYS older
than the staging in the tree, and the freshness probe is right to say so.

**Decisive test:** `just nuttx build-fixtures-arm` alone → the arm entry is
FRESH relative to `staging/libc.a`, and all three nuttx action Runtime cells
(C, C++, Rust) pass. The interleaving is the whole defect.

## Status

Root-caused and worked around, not structurally fixed. Building one arch at a
time converges; `build-fixtures` (both) does not. The structural options are a
per-arch kernel tree, or a freshness signature that keys on the arch-specific
export rather than the shared `staging/` dir. Both are larger than this issue
and belong with whoever owns the NuttX board work — leaving this resolved-with-a-
workaround rather than claiming the interleaved build is fixed.

## Fix method, explored (2026-08-05)

### What the code already knows

The shared staging dir is not an oversight — it is declared:

* `nuttx_ffi_build.rs:453` — "The staging dir is SHARED between configs (arm and
  riscv kernels both stage into `third-party/nuttx/nuttx/staging`), so the lib
  set this scan sees can change under a cached build-script output." It defends
  with `cargo:rerun-if-changed=<staging>`.
* `scripts/build/fixture-inventory.py` declares it formally:
  `shared_mutation: "$NUTTX_DIR/staging/libc.a; $NUTTX_DIR/include/nuttx/config.h"`.

That `rerun-if-changed` is exactly what puts the staging dir into each entry's
`.d`, and the test-side freshness probe reads that `.d`. So the probe is
downstream of a defence, not missing one.

### Rejected: exempt the shared kernel in the probe

Tempting, and there is precedent — `dep_info_newer_source` already exempts two
classes of build-side-effect mtime (`REGENERATED_INPLACE_HEADERS` for issue #222,
`is_cargo_out_dir_product` for phase-300).

**But it is wrong here.** Those exemptions are safe because the exempted file's
content cannot differ semantically without an edited source that IS in the dep
graph. The shared staging dir fails that test: after a riscv build it holds
riscv archives, so an arm entry relinked against it would link the WRONG kernel —
which is precisely what `nuttx_ffi_build.rs`'s comment says it is defending
against. The probe is telling the truth. Silencing it would let a test run a
binary whose link inputs are from another architecture.

### Recommended: per-arch staging snapshot

Give each architecture its own copy of the staged archives, so one arch's build
cannot invalidate the other's entries:

1. `scripts/nuttx/build-nuttx.sh` — after the kernel build, snapshot
   `staging/` to `staging-<arch>/` (the script already derives `CONFIG_ARCH`
   for its run hint).
2. `packages/boards/nros-board-common/src/nuttx_ffi_build.rs` — resolve
   `staging-<arch>` when present (fall back to `staging`), and watch that path
   instead.
3. `packages/boards/nros-board-common/src/nuttx_image_link.rs` — same path
   resolution (`nuttx_dir.join("staging")` at line 108).

This fixes the root cause rather than the symptom: it also removes the
"lib set can change under a cached build-script output" hazard the build script
currently only DETECTS, and it restores the build-once-link-many property
`build-nuttx.sh` claims in its own comments.

The version-keyed export dir (`nuttx-export-<ver>/libs/`) carries the same
archives and was considered as the snapshot source, but it is not arch-keyed and
`make export` wipes it per run — so it collides exactly like `staging/`.

### Not applied here

This changes the link path for every NuttX fixture on both architectures, in the
board area phase-337 is actively reshaping, and verifying it means rebuilding
both arches. The workaround (build one arch at a time) is sufficient for the
RFC-0069 acceptance this was blocking, so the structural change is left for the
NuttX board owner with the method above rather than landed opportunistically.
