---
id: 2255
title: "The book is published from `main` and teaches CLI verbs the latest release does not have (`nros dev`, `nros run`, `nros new --board`)"
status: resolved
type: bug
area: book, release, cli
purpose: [industrial, education]
related: [phase-523, rfc-0116]
found: 2026-11-05
resolved_in: "release nros-0.6.0-nros7, cut from main 538ccb9084 on 2026-11-06"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round one (2026-11-05); round two counts what is left.

Found by phase-523 W3 adoption probe round one, 2026-11-05, on release `nros-0.6.0-nros6` (2026-10-31, 318 commits behind `main` at the time) in a pristine `ubuntu:24.04` container, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n.

## Measured

- I1/E1, installation.md "Check the install": `nros dev sdk root` → `error: unrecognized subcommand 'dev'`.
- I2/E3, first-project.md "Scaffold the talker": `nros new sensor_node --board mps2-an385-baremetal --template talker` → ``Error: `nros new <name>` requires `--platform <p>` ``; with `--platform baremetal`, `--board` is refused as "for a C/C++ project". The generated tree also differs from the page (no README, extra `nros.toml` and `.cargo/config.toml`).
- I3/E2, first-project.md "Run it": `nros run --expect 'Publishing:'` → `error: unrecognized subcommand 'run'`. The education probe's grading command for every lab is this verb.
- I18: `install.sh` is always fetched from `main`, even when a version is asked for.

All three verbs landed after the release was cut: `nros dev` d1c63c3021 (11-01), `nros new --board` 943b39fc2e (11-01), `nros run` 30bcf1c18a / 9d00c1e5c5 (11-03/04). The published book is built from `main` (docs.yml), so a reader who installs the newest release follows a book about a CLI they do not have, and no page says which release it describes.

## Next

Either the release keeps up with the book (cut a release after each user-visible CLI change, or before the book deploys it), or the book states the release it describes and is published per release. A release now would clear this one probe round's three blocking stops; the second is the structural fix.

## Resolution (2026-11-07)

Release `nros-0.6.0-nros7` was cut from `main` at `538ccb9084` (2026-11-06,
[workflow run](https://github.com/NEWSLabNTU/nano-ros/actions/runs/42276831597)),
on the maintainer's instruction. It carries every round-one fix. Installed in a
pristine `ubuntu:24.04` container with the book's own `install.sh` line, it
answers every verb this issue measured missing and the flags the book now
teaches:
`nros run`, `nros dev sdk path qemu` (prints the store's
`qemu/11.0.0-nros8`), `nros new --board`, `nros configure`,
`nros setup --offline-dirs`, `nros build --explain-sizing` and
`nros doctor --report`. `nros-sdk-index.toml`'s `[tool.nros]` pin moves to it in
the same change that closes this issue.

The class is not gone, only reset: the book is still published from `main`, so
it runs ahead of the newest release again with every merged verb. Support
status now says it describes `main` and names `nros doctor --report` for the
installed release (issue 2267), and the release-support policy is "latest
only" (issue 2268), so the remedy for a gap is the next release.
