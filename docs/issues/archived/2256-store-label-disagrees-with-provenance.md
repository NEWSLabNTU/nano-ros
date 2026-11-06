---
id: 2256
title: "`nros setup` labels a store source dir with the fetched index's pin while filling it with another (`zenoh-pico/1.10.1+c78dbffd` holds `8d5ac2fe`)"
status: resolved
type: bug
area: cli, setup, store
purpose: [industrial, education]
related: [phase-523, rfc-0116]
found: 2026-11-05
resolved_in: "issue-2257 branch: a release reads its shipped index (RFC-0097 D5 amended)"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round one (2026-11-05); round two counts what is left.

Found by phase-523 W3 adoption probe round one, 2026-11-05, on release `nros-0.6.0-nros6` (2026-10-31, 318 commits behind `main` at the time) in a pristine `ubuntu:24.04` container, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n.

## Measured

- I4/E4, bare-metal.md "Build": `nros build` → `zenoh-pico source not provisioned at "/home/dev/.nros/sdk/nros/0.6.0-nros6/share/nano-ros/packages/rmw/zenoh/zpico-sys/zenoh-pico". Run: nros setup --source zenoh-pico …`. That command answers `[already present (skip)]` and the build fails the same way; `nros doctor` reports all OK. Reproduced on two seats.
- I5: the store dir `zenoh-pico/1.10.1+c78dbffd` (the label from the index `nros setup` fetched from `main`) contains the `8d5ac2fe` tree (`.nros-provenance ref = "8d5ac2fe…"`, the release's pin).

So the store's directory name, the provenance file inside it and the pin the build asks for are three different answers. The build reads the release's shipped pin, setup provisions from the fetched index (issue 2257), and the label/content mismatch means no lookup by either pin finds it.

## Next

Check on `main` that a store source dir's label is derived from the same pin as its contents. Make the build's "not provisioned" refusal name the pin it wanted and the pin the store holds, so the advice cannot loop.

## Resolution (2026-11-06)

Root cause shared with issues 2256–2258: `nros setup` on a release read
`main`'s fetched index, while every other reader (`qemu_bin::for_sdk`, the
build's source preflight, `nros locate`, the drift check) read the release's
own SDK root, about 20 sites. The maintainer ruled that a release uses its own
index, which amends RFC-0097 D5. `setup::release_reads_shipped` makes a
released `nros` answer the store rung with its shipped
`share/nros/nros-sdk-index.toml`, the same file the SDK root carries, and
fetch nothing unless `$NROS_INDEX_URL` names a mirror. A binary with no shipped
copy keeps the fetch path. Unit-tested; round two re-measures it on the
release that carries the change.

The label came from the fetched index's pin while the tree came from the
release's (`8d5ac2fe`), so the two disagreed. One index gives one pin for both,
and the build's "not provisioned" check finds the dir that setup filled.
