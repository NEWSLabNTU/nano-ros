---
id: 2334
title: "`nros version` prints `0.6.0`, hiding the `-nrosN` release suffix the installer and pin use"
status: resolved
type: bug
area: cli
purpose: [education, industrial]
related: [phase-523, issue-2328]
found: 2026-11-07
resolved_in: "adoption-r2-cli-ux branch: nros version and doctor --report print the store version; a checkout build says it has none"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md)** — W3 round two. Found 2026-11-07 on release `nros-0.6.0-nros7` in a pristine `ubuntu:24.04` with Jazzy, reading only the book. Probe ids: `I<n>` = industrial stop n, `E<n>` = education stop n (runs: `docs/development/adoption-probes/runs/2026-11-07-round-2-{industrial,education}.md`).

## Measured

- E8: the installer says `installed 0.6.0-nros7`, the pin must say `version = "0.6.0-nros7"`, and `nros version` prints `nros 0.6.0`. An instructor checking seats cannot tell `-nros6` from `-nros7` with it. `nros pin` shows the suffix.
- I12: `nros doctor --report` prints `nros 0.6.0` too.

## Next

Print the full release version (the index pin's `version`) in `nros version` and in doctor.

## Resolution (2026-11-07)

`nros version` and `nros doctor --report` printed `env!("CARGO_PKG_VERSION")`.
Both now print `cmd::version::release_version()`, which takes the store
version from `nros_launcher::pin::running_version`, the reader `nros pin`,
`nros doctor`'s `running` line and the launcher's dispatch already use (the
store prefix `sdk/nros/<v>`/`toolchains/<v>`, else `share/nros/VERSION`). No
second source was added. A release prints `nros 0.6.0-nros8`. A checkout
build has no release version and says so, with its source stamp:
`nros 0.6.0 (checkout build, no release version; sources 0d722c0f5398)`.
`nros --version` is unchanged, because `install.sh` and the release workflow
read it as one line. Unit-tested on both arms and through a store-prefix path;
the Install page's example output now shows the suffix.
