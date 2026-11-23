---
id: 2912
title: "Nothing tests the upgrade from one release to the next, so the release page's upgrade steps are measured by hand"
status: open
type: test-gap
area: [testing, ci, docs]
severity: medium
found: 2026-11-24
---

## What happens

Each release page in `book/src/migrating/releases/` tells a user what to edit
and what an old project or build directory does after the upgrade. The release
workflow publishes that page as the GitHub release body. No lane checks it.

The book probe's installed track (`just probe <track>`) installs ONE release
asset into a clean container and replays a chapter. It cannot install the
previous release, build a project with it, and then upgrade. So the upgrade
steps on a release page are the one part of the book that no probe reaches.

For nros12 (#4724) the page's claims were measured by hand on copies of
`examples/linux/rust/talker` with the CLI from `main`:

- an old setting name in `system.toml` (`[image.linux.knobs.zenoh.wire]
  frag_max_size`) is refused, naming `[image.linux.knobs.zenoh]
  frag_max_bytes`;
- `NROS_DOMAIN_ID=5 nros build` stops, naming `[system] domain_id`;
- `executor.backing_u64s` is refused with "the value x 8".

That checks the new release's refusals on a fresh project. It does not check
what the page says about a project and build directory made with the previous
release, for example:

- a Zephyr build directory that cached a retired `-DCONFIG_` value is
  re-configured with a warning that drops it (#4719);
- a C or C++ image's build directory moves to one named for the image, and
  `nros build` removes the old one;
- the page's opening line, that a project generated with the previous release
  needs no regeneration.

Each of these depends on state the previous release wrote, which is exactly
what a fresh install never has.

## What would close it

An installed-track probe that tests the upgrade:

1. Install the previous release's asset (the tag before the candidate) into a
   clean container.
2. Create and build the projects the release page names. At least one Rust and
   one C or C++ host image. A Zephyr image too, when the container has its SDK.
3. Install the candidate asset over it, the way a user upgrades.
4. Rebuild without editing anything, and assert the warnings and refusals the
   release page documents. The page's output blocks are the assertion, as
   `probe-expect` already is for a chapter.
5. Make the page's edits, rebuild, and assert the build passes.

The release page then holds probe-tagged steps for the upgrade, like a
chapter, and the release gate runs the track against the candidate before the
cut.

Not an nros12 blocker: the nros12 page quotes hand-measured messages.

## Related

- `scripts/probe/run-bootstrap-probe.sh` and `just/probe.just` (the installed
  track this would extend)
- `book/src/migrating/releases/0.6.0-nros12.md`
