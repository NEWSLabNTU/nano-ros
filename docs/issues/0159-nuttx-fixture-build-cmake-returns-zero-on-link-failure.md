---
id: 159
title: "NuttX fixture build can report success with no kernel ELF — `cmake --build <entry>_build` returns 0 on some link failures"
status: open
type: tech-debt
area: build
related: [phase-281, issue-0149]
---

## Summary

A NuttX workspace/example fixture can finish its build with exit 0 yet produce
**no bootable kernel ELF**. Surfaced during phase-281 W3-nuttx: a link failure
was only detected by explicitly checking for the artifact, not by the build's
exit code.

## Mitigation landed

`scripts/build/workspace-fixtures-build.sh` now **fails loud** for NuttX when the
build produces no `<entry>` kernel ELF (empty artifact-find → `return 2` instead
of the old lenient "built target" branch that still stamped a non-existent
fixture) — commit `7ad8cc884`. This is a robust backstop: any missing-ELF case
now fails the build, regardless of root.

## Untracked root

The backstop does not explain WHY `cmake --build --target <entry>_build` can
return 0 with no artifact. The final flat-build link goes through cargo/rustc
(`nros-nuttx-ffi`'s build.rs emits `cargo:rustc-link-arg` + the cc-rs archives;
the cargo binary IS the kernel image), and cargo/rustc + cc-rs `.compile()`
propagate their own failures — so most link errors *do* surface a non-zero exit.
The observed exit-0 edge is therefore likely one of:

- a cmake **up-to-date skip** (a stale/absent `_output_binary` whose `DEPENDS`
  didn't change, so the custom command isn't re-run), or
- a sub-step inside the custom command whose failure isn't propagated
  (`${_provision_cmd}` prefix, or an env/cc step).

## Direction (optional — the backstop covers the failure)

If it recurs, instrument the `nros_nuttx_build_example` custom command
(`packages/core/nros-c/cmake/nros-nuttx.cmake`) to `BYPRODUCTS` the ELF + verify
it inside the command (fail the command, not just the outer script), so
`cmake --build` itself is honest. Low priority — the workspace-fixtures backstop
already prevents a broken fixture from being stamped as built.
