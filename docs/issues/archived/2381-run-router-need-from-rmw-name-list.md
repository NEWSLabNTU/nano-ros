---
id: 2381
title: "`nros run` decides the host router from a list of RMW names, so an out-of-tree RMW gets none"
status: resolved
type: bug
severity: medium
area: [cli, rmw, runners]
related: [phase-545]
found: 2026-11-09
resolved: 2026-11-10
resolved_in: "PR #3382: the host peer is a fact the RMW descriptor declares, `[rmw.host] peer` (RFC-0071 D17), and `nros run` reads it, never the RMW's name"
---

**Home: [phase-545](../../roadmap/phase-545-rmw-provider-scan-build-road.md) W3.**
Found with W3's out-of-tree fixture (`rmw = "acme"`, PR #3376).

## Symptom

`nros run` chose what to start on the host side of a link in
`run_exec::host_side` from the RMW's NAME:
- `xrce` got an XRCE agent;
- `zenoh`, or an unset RMW, got a zenoh router;
- every other name got nothing.

So the out-of-tree `acme` RMW, which delegates to zenoh, ran with no router
under `--router auto`. The book's custom-RMW page had to start
`rmw_zenohd` by hand.

The name list is the in-tree table again, one layer up. Phase-545 removed
that table from `nros build` (RFC-0071 D14 to D16), and `nros run` kept its
own copy.

## Resolution

Resolved by PR #3382 (2026-11-10).

- **The declared fact.** The host peer an RMW needs is now a fact its
  descriptor declares (RFC-0071 D17):

  ```toml
  [rmw.host]
  peer = "zenoh-router"   # or "xrce-agent", or "none"
  ```

  `nros run` reads it, never the RMW's name.
- **In-tree descriptors must state it.** `cargo-nano-ros/build.rs` and
  `check-rmw-descriptors` S3 both refuse a missing value: zenoh
  `zenoh-router`, xrce `xrce-agent`, cyclonedds and uorb `none`.
- **Out-of-tree, a missing value is unknown, not `none`.** `--router auto`
  refuses with exit 3 and names the line to add.
- **The plan carries the peer.** The run plan resolves the image's RMW
  through `rmw_index::for_workspace`, the same index the build uses, and
  puts it on the link as `host_peer`, with provenance. `host_side`'s
  `None => zenoh` default is gone, because the plan's RMW is already
  resolved at the build's parse boundary.
- **acme** declares `zenoh-router`.
- **Gate:** `run_exec::tests::the_run_path_names_no_rmw` refuses an RMW name
  as a string literal in the run path.

**Not measured here:** an `nros run` of the acme fixture itself. On main its
build fails first, at issue 2385, the `_ws` generated path regression of
#3352. Re-run it once that revert lands.
