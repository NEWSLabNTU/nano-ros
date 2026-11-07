---
id: 2321
title: "`nros-sdk.lock` omits the Rust toolchain, the source-built packages and corrosion's hash, and a workspace lock's `[tool]` is empty"
status: resolved
type: enhancement
area: setup, provenance
purpose: [industrial]
related: [phase-523, issue-2267, issue-2333]
found: 2026-11-07
resolved_in: "adoption-r2-provenance branch: source-built tools lock their origin; the lock records the Rust toolchain"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round two. Found 2026-11-07 on release `nros-0.6.0-nros7` in a pristine `ubuntu:24.04` with Jazzy, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n (runs: `docs/development/adoption-probes/runs/2026-11-07-round-2-{industrial,education}.md`).

## Measured

- I4: after `nros setup mps2-an385-baremetal --rmw zenoh`, the lock has sha256 for arm-none-eabi-gcc and qemu, but records corrosion as `provenance = "source"` with no hash. It has no entry for rustup, Rust 1.99.0, zenoh-pico or mbedtls (doctor shows only `mbedtls 3.x` plus a commit).
- The workspace `nros-sdk.lock` has an empty `[tool]` table, because the tools were recorded in the lock in the cwd (issue 2333).
- Install › Working offline says "the index names every asset's upstream". No page shows the index, and it lives inside the SDK root.
- Setup output contains raw `cargo:rerun-if-env-changed=…` lines.

## Next

A reviewer must be able to rebuild the provenance list from the lock alone. Record the toolchain channel and its components, every source package (URL, commit and tree hash), and the source-built tools' output hashes. Then show the lock in the book.

## Resolution (2026-11-07)

- A source-built tool records where it came from: the git remote and commit,
  or the tarball URL and its sha256. That lands in its `.nros-provenance` and in
  the lock (`source = …`, `ref = …`), so corrosion is no longer
  `provenance = "source"` with nothing to re-fetch. A prefix installed before
  this records its origin the next time it is installed.
- `nros setup` records the default compiler in a `[rust]` table: `release`,
  `commit` and `host` from `rustc -Vv`.
- Test: `a_source_build_locks_where_it_came_from`.
- An empty workspace `[tool]` happened because setup wrote the lock into the
  working directory; issue 2333 covers that. The raw `cargo:rerun-if-env-changed`
  lines in setup output were not reproduced on `0.6.0-nros8`.
