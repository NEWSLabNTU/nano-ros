---
id: 2472
title: "The `linux-rust-zenoh-logging` fixture row claims rmw `zenoh`, but its owned entry links no RMW, so `rmw_coordinate_truth` fails on it"
status: open
type: bug
severity: medium
area: [tests, fixtures, examples]
found: 2026-11-12
related: [issue-0831, issue-2434]
---

## What

`nros-tests::rmw_coordinate_truth a_rows_rmw_is_the_backend_its_artifact_linked`
fails on main 8cba9c078d42ebaafded38bf13ee7747f9ac7bcc, in the tier-1 run and again run solo:

```
1 artifact(s) do not link the RMW their fixture row claims and 0 could not be read (checked 162):
  linux-rust-zenoh-logging: row declares rmw `zenoh`, but
  examples/linux/rust/logging/target-fixtures/nros-relwithdebinfo/native_logging links none of it
```

The row (`examples/fixtures.toml`) is a `[[workspace_fixture]]` with
`rmw = "zenoh"`. Its image's entry is the owned entry `src/native_logging`,
whose `Cargo.toml` depends only on `nros-log` and `nros-platform-cffi`: it
drives the log sink and opens no session. The bringup says so itself:

```
# ... opens no session, so the bringup declares no component
# table. `rmw` is stated because every image names one; nothing here links it.
```

So the row's coordinate claims a backend the program deliberately does not
link. The test (issue 0831) is doing its job.

The leaf was migrated by hand in cbd799c254fde20776bf2d8b00556912d245315a (phase-535 W6; the hand migration
is issue 2434). `nros configure` also prints, for this entry:

```
sync: entry `native_logging` names neither `nros` nor an `nros-board-*` crate,
so no adapter is written for it and its image's features reach it from nowhere.
```

## Evidence that it is not a stale binary

The artifact checked is from 2026-11-11 23:06 UTC, older than HEAD (the
workspace fixture stage could not rebuild it at 8cba9c078d42ebaafded38bf13ee7747f9ac7bcc: a `road: unbound
variable` stop in `workspace-fixtures-build.sh`, since fixed on main by
fcd3ffb32427b4fe5966b1cc245fd173c04a7b3f). That does not
matter here: the leaf sources predate the artifact (last change cbd799c254fde20776bf2d8b00556912d245315a),
and the manifest has no RMW dependency, so a fresh build links no RMW either.

## Fix direction

One of:

- the row (and the coordinate machinery) can say "no RMW" for an image that
  opens no session, and the test accepts that; or
- the example links an RMW it does not use, which would be a worse example; or
- the logging row leaves `[[workspace_fixture]]` coverage of
  `rmw_coordinate_truth`, with the reason written next to the row.

Pick by what `row_coord()` and the lane selection need from a row without a
backend; "every image names one" is the assumption to check.
