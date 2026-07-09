---
id: 168
title: "zenohd install location split-brain — `just native zenohd` broken, three docs give three conflicting launch commands"
status: open
type: bug
area: build
related: [rfc-0014, phase-277]
---

## Problem

The first-run happy path (README Quick Start) breaks at the "start the router"
step because zenohd lands in **two different locations** depending on which
setup route the user took, and the three run docs each assume a different one:

| Route | Install location |
| --- | --- |
| Contributor: `bootstrap.sh base` → `just setup base` → `just zenohd setup` | `build/zenohd/zenohd` |
| User: `nros setup native --rmw zenoh` (README step 4) | `~/.nros/sdk/zenohd/<ver>/bin/zenohd` |

Doc drift on top:

- `just/native.just:779` (`just native zenohd`) runs **bare `zenohd`** — but
  nothing puts zenohd on PATH. `activate.sh:78-94` deliberately only adds SDK
  `bin/` dirs containing `*-gcc`, `genromfs`, or `sccache`; `scripts/sdk-env.sh`
  exports no zenoh var. Result: after following the README verbatim,
  `just native zenohd` fails "command not found".
- `examples/native/README.md:29` quick-start uses `just native zenohd &` →
  fails as above.
- `examples/README.md:260` uses `build/zenohd/zenohd` → only exists on the
  contributor route.
- Root `README.md:86-87` hand-rolls a PATH export over `~/.nros/sdk/zenohd/*/bin`
  → the only form matching the README's own setup step, and it contradicts the
  same README's claim (line 44) that `nros setup` means "no manual build step".
- Following the README top-to-bottom installs zenohd **twice** (once via
  `just setup base`, once via `nros setup native`).

## Fix direction

1. Make `just native zenohd` self-contained: resolve the binary inside the
   recipe (`~/.nros/sdk/zenohd/*/bin/zenohd`, falling back to
   `build/zenohd/zenohd`) instead of assuming bare PATH.
2. Have all three docs (`README.md`, `examples/README.md`,
   `examples/native/README.md`) use that single `just native zenohd` line.
3. Optionally: `activate.sh` adds whichever zenohd location exists to PATH, and
   the README drops the duplicate install.
