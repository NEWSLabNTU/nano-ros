---
id: 2864
title: "A Zephyr image's os_config entries resolve from two different bases: .conf/.overlay from the west application first, the board network file from the bringup"
status: resolved
type: tech-debt
severity: low
area: [cli, zephyr, docs]
related: [issue-0892, rfc-0085, rfc-0109]
found: 2026-11-23
resolved: 2026-11-23
resolved_in: "fix/west-entry-and-run-gaps"
---
Found by a downstream safety-island port, whose bringup comment says its
`os_config` paths "resolve from the west application's directory"; seen on
main 545ae12ec1.

## What

One key, `[image.<id>] os_config`, lists files of two kinds, and each kind
has its own base:

- a Zephyr fragment (`.conf`, `.overlay`, `.dts`) is looked for at
  `<bringup>/boards/<board>/<name>`, then `<west app>/<name>`, then
  `<bringup>/<name>`, first hit wins (`builder/zephyr.rs`,
  `resolve_fragment`; the application rung is issue 0892's);
- a board network file (`.toml`) is read from `<bringup>/<name>` only
  (`system_net::fold_board_net_files`, "resolved against the system.toml's
  directory").

For a generated application the app rung is the generated directory, which
holds no user file, so the bases agree. For an owned entry they differ: a
relative fragment path is read from the ENTRY package first, a network file
beside it from the BRINGUP. A same-named file in both places is taken from
the entry without a word.

The book says the network file is "named by `os_config`, beside
`system.toml`" (`book/src/reference/system-toml.md`) and says nothing about
where a fragment is looked for.

## Repro

An owned-entry image with `os_config = ["prj-x.conf"]` and a `prj-x.conf` in
both the bringup and the entry package builds with the entry's copy; delete
the entry's copy and it builds with the bringup's. In the port, the three
paths are `../../controller_bringup/...`, which reach the same directory from
`src/test_bringup/unit_test` and from `src/asi_tests/asi_unit_test_zephyr`, so
it cannot tell which base it got.

## Resolution

By design (RFC-0085 D5 for fragments, RFC-0109 D6 for the network file), so
nothing is refused: a same-named fragment at a higher rung is a board's or an
application's override. Documented side by side in
`book/src/reference/system-toml.md` ("Where `os_config` files are found"),
which the Zephyr west-app guide's Configure section links. `nros build` now prints the rung each fragment
came from (`ZephyrOverlays::fragment_origins`); test
`builder::zephyr::tests::each_fragment_reports_the_rung_it_was_found_in`.
