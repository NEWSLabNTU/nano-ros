---
id: 2495
title: "The NuttX make road rebuilds nano-ros only on the first build: an edited Rust source or project setting reaches the archive only after a clean"
status: resolved
type: bug
area: [nuttx, build]
severity: medium
found: 2026-11-13
related: [issue-2236, issue-2426, phase-550]
---

**Home: [phase-550](../../roadmap/phase-550-nuttx-native-build-infrastructure.md) W6b**

## Symptom

On the NuttX make road (`apps/external/nano-ros`, `integrations/nuttx/`),
the nano-ros archive is rebuilt only by the first `make` after a configure
or a clean. After an edit to a nano-ros Rust source, or (once a project is
named) to its `system.toml`, the next `make` links the archive it already
has. It reports success, so the image is a stale build that looks current.

## Why (measured, 2026-11-13)

The shell runs cargo from its `context::` recipe. NuttX reaches an app's
`context` through one stamp per apps tree. `tools/Unix.mk` at the pinned
kernel:

```make
%.context: include/nuttx/config.h .dirlinks
	$(Q) $(MAKE) -C $(patsubst %.context,%,$@) TOPDIR="$(TOPDIR)" context
```

The stamp `apps/.context` depends on `include/nuttx/config.h` and
`.dirlinks` alone. Once it exists, `make` never enters any app's `context`
again until a clean (`clean_context`, `distclean`) deletes it.

Measured in a staged `qemu-armv7a` tree that had built once. A second
`make` with a newly named project (`NROS_PROJECTS`) ran no `context::`, no
`nros configure` and no cargo. It ran them only after `apps/.context` was
removed by hand.

## Why nothing caught it

Every lane built this road from a clean tree. `just nuttx build-make-road`
(phase-550 W1) clones fresh trees on every run, so it only ever exercises a
first build.

## Fix direction (agreed with the manager session, 2026-11-13)

- Run `nros configure` (a digest check when nothing moved) and cargo on
  EVERY build, from a phony prerequisite of the app's own archive or library
  rule, outside `context::`. Cargo is then its own freshness oracle.
- Do not delete `.context`: that fights NuttX's mechanism and re-runs every
  app's `context`.
- Do not hand-list input files: such a list drifts (issue 0627's lesson).
- Measure what the no-op rebuild adds to each `make`.
- Add an incremental case to the lane: build, edit, build, and assert that
  the archive changed.

## Fix (2026-11-13, phase-550 W6b)

The steps that make the archive (the object dirs, the image crate, `nros
configure` when a project is named, then cargo) are one `define` in
`integrations/nuttx/Makefile`, run from `context::` (the first build) and
from `depend::` (every build). NuttX runs `make -C $(APPDIR) depend` on every
build through `pass1dep`, a prerequisite of `libapps`, and it finishes before
any app compiles. Cargo decides what is stale. Nothing deletes `.context`,
and nothing lists input files.

Measured in a staged `qemu-armv7a` tree:

- touching a nano-ros Rust source rebuilt `libnros_c.a` on the next plain
  `make` (7.5 s), where before the fix nothing ran;
- a no-op `make` takes 1.9 s with the fix against 1.42 s without it, so
  about 0.5 s more per `make` (the image-crate check, the settings file and a
  no-op cargo).

`just nuttx build-make-road` now checks this on every run. After the first
build it touches the generated image crate's source (a build-dir file, so no
checkout mtime moves), runs `make` again, and requires `libnros_c.a` to
change. With the `depend::` hook removed, that check fails, naming the
archive.
