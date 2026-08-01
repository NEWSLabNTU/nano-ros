---
id: 373
title: "The book's install path is exercised only on ubuntu+bash — on Arch Linux three of its steps are wrong or unactionable"
status: resolved
type: tech-debt
area: build
related: [rfc-0014, rfc-0062, issue-0204, issue-0368, issue-0372, issue-0383, phase-327]
resolved_in: "issue-0373 fix (see git log for `probe=NN distro=`)"
---

# The book's install path is exercised only on ubuntu+bash

`book/src/getting-started/installation.md` is the user-facing front door, and
its only executable coverage — `just probe bootstrap` (issue 0204) — ran
`ubuntu:24.04` under `bash`, with `PROBE_IMAGE` overridable but never
overridden and no non-bash lane anywhere. Walking the page on Arch Linux with
zsh surfaced three defects the probe could not see.

## Findings and resolution

**F1 — the prereq block was apt-only.** Fixed: the page now carries
Debian/Ubuntu, Fedora/RHEL, Arch and macOS blocks (noting `base-devel` is a
group and covers `pkg-config`), and points at `nros setup --system` for the
per-board OS deps, which already resolves apt/dnf/pacman/brew from the
`[system.*]` index class (phase-327).

**F2 — `just` was a de-facto prereq while the page said otherwise.**
`activate.sh` sources `scripts/sdk-env.sh` unconditionally, which shelled out
to `just` and printed a bare `SDK defaults not loaded`. Fixed: the message now
says WHICH defaults (`FREERTOS_DIR`, `NUTTX_DIR`, `IDF_PATH`, …), that they are
harmless for the native flow and needed for embedded builds, and names the
remedy. installation.md carries the same note where it claims `just` is not a
prereq — the claim is true for the user path, it just read as a half-failure.

**F3 — the ROS 2 warning was unactionable and the book never scoped ROS.**
Fixed: installation.md gains a "Do I need ROS 2 installed?" table, and
`activate.sh`'s warning names what actually breaks (interop tests, `ros2` CLI
verification) and what does not. The scoping is empirical, not assumed — on
this ROS-less host `nros sync` generated the bindings from
`packages/cli/interfaces/` and the Rust talker published end to end against the
store `zenohd`.

## Coverage, so the next one is caught

The root cause was that only one host shape was ever probed. Now:

- `probe=NN` blocks accept a `distro=` token; `extract-book-steps.py --distro`
  selects them. A requested distro with no block for a tagged step is a hard
  error, not a silently shorter probe (verified: `PROBE_DISTRO=gentoo` fails
  naming which distros step 10 exists for).
- `run-bootstrap-probe.sh` grew `PROBE_DISTRO` (apt/dnf/pacman container shim)
  and `PROBE_SHELL`.
- `just probe bootstrap-arch` / `bootstrap-fedora` / `bootstrap-zsh`. Opt-in,
  not in any tier — the shapes to run before a release or after touching
  `activate*` / `bootstrap.sh`.

Extraction verified for debian/arch/fedora; the container runs themselves are
opt-in and were NOT executed here (they need docker and ~30-60 min each).

Running the Arch path by hand also turned up a separate hard blocker that this
issue did not predict — vendored zenoh-pico fails to compile on gcc >= 14 —
filed as issue 0383.
