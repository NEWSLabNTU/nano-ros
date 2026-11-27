---
id: 3002
title: "`nros config explain` searches the whole directory tree it runs in for platform packages, and one unreadable file there stops it"
status: resolved
type: bug
severity: medium
area: [cli, ux]
found: 2026-11-27
resolved: 2026-11-27
resolved_in: "fix-3002-cwd-not-a-workspace"
---

## What happens

`nros config explain --platform <p>` takes the current directory as the
workspace whose platform packages it searches (`--workspace`, "default: the
current directory"). It does that whether or not the directory is a
workspace, and it searches the whole tree below it for
`nros-platform.toml`.

Measured on main at 22886a4c2c, host newslab-server139, 2026-11-27:

- **From `/tmp`:** the platform search path held over 300 directories,
  every `nros-platform.toml` left under `/tmp` by test runs
  (`/tmp/nros-cli-core-tests-*/configure-stamp-*`, `/tmp/tmp.*/checkout/config`,
  `/tmp/nros-platform-watch-*`). One of them is not valid TOML, so the command
  stopped with:

  ```text
  Error: load platforms from /tmp/nros-cli-core-tests-1022920/…:/tmp/nros-platform-watch-3360484-pkg:…: /tmp/nros-platform-watch-3360484-pkg/nros-platform-nuttx/nros-platform.toml: TOML parse error at line 1, column 2
  ```

  The error line is several kilobytes long, and the file it names has
  nothing to do with what the user asked.
- **From a scratch directory** holding a platform package scaffolded
  earlier with `nros new platform myos`, the command used that directory
  as its "platforms root" without saying so, though no workspace was
  there.

A user who runs it from `$HOME` searches their whole home directory.

## Expected

- Search a workspace only when the directory is one: the same workspace
  detection `nros build` uses. Elsewhere, search only the installed
  release's and the checkout's platforms, and say so.
- An unreadable `nros-platform.toml` outside the workspace being used is
  never read. One inside it is reported on its own, without the whole
  search path.

## Related

- `packages/cli/nros-cli-core/src/cmd/config.rs` (`ExplainArgs::workspace`,
  `resolve_ladder`).
- Found by the book's CLI sweep (2026-11-27), while checking
  "Tune throughput and latency"'s `nros config explain` output.

## Fix (2026-11-27)

One rule decides whether the current directory is a workspace,
`image_facts::enclosing_workspace`. It accepts the nearest ancestor whose
`src/` holds a bringup. It also accepts the directory itself when it holds a
`system.toml` (a leaf) or when its `src/` holds a package (`package.xml`,
`Cargo.toml` or a platform descriptor; `nros new platform` scaffolds no
`package.xml`).
- `config explain` searches the workspace rung only when that rule finds a
  workspace (or `--workspace` names one). Elsewhere it searches nano-ros's
  own platforms and RMWs. `knob list` builds its catalog the same way:
  outside a workspace, from the nano-ros packages alone, never from the
  current directory.
- The completion cache reader and `rmw_index::for_dir` each spelled a
  narrower form of the same rule; both call the helper now.

Negative control, from a directory holding a broken `nros-platform.toml` two
levels down: a binary from main exits 1 with the load error naming that file;
the fix exits 0 and never names it
(`config_explain_workspace_platform::outside_a_workspace_nothing_below_the_directory_is_read`).
The porter's case keeps working: a workspace holding only a scaffolded
platform package is still searched (`a_scaffolded_platform_explains_from_its_workspace`).
