---
id: 372
title: "`activate.sh` aborts mid-file under zsh — unmatched SDK globs are fatal, so the second half of the activation never runs"
status: open
type: bug
area: build
related: [rfc-0014, issue-0204, issue-0368, phase-218, phase-327]
---

# `activate.sh` aborts under zsh on unmatched SDK globs

## Summary

`source ./activate.sh` **fails partway through on zsh**, both before and after
`nros setup`. zsh's default `nomatch` option makes an unmatched glob a fatal
error, and `activate.sh` has two `for`-loops that glob into the SDK store. The
sourced shell dies at the first one, so every export below that line — the SDK
store PATH loop (which is what puts `zenohd` on PATH for the book's first-node
flow), pinned ninja, pinned make, `.env`, and `scripts/sdk-env.sh` — never runs.

The file's own header advertises the opposite on both counts:

- line 1: `# nano-ros workspace activation — POSIX shell (bash / zsh).`
- line 15: `# corresponding binaries / SDKs are absent, the export is harmlessly
  skipped — the script never errors.`

bash is unaffected (an unmatched glob stays literal; the `[ -x … ]` / `[ -d … ]`
guards then skip it as intended). This is a zsh-only failure and it is silent in
the sense that matters — the user sees one stray line, gets a shell prompt back,
and only discovers minutes later that `zenohd` is not on PATH.

## Evidence

Arch Linux host, `$SHELL=/usr/bin/zsh`, zsh 5.9.2, fresh checkout, canonical
book flow (`./scripts/bootstrap.sh` → `source ./activate.sh`).

**Before provisioning** (`~/.nros` does not exist) — dies at line 92:

```
$ source ./activate.sh
activate.sh: /opt/ros/humble/setup.bash not found — ROS-dependent recipes will fail
./activate.sh:92: no matches found: /home/aeon/.nros/sdk/play_launch_parser/*/bin
EXIT=126
```

`NROS_REPO_DIR` and the `nros` PATH entry (lines 27–78) do get set, because they
precede the failure — which is why the breakage reads as "activation worked".

**After provisioning** — line 115 fails too, for a different reason. The loop
words are two patterns:

```sh
for _nros_tcbin in "$_nros_sdk"/*/*/bin "$_nros_sdk"/*/bin; do
```

zsh applies `nomatch` **per word**: if *either* pattern matches nothing, the
whole command errors. `nros setup` writes the versioned layout only
(`sdk/<tool>/<version>/bin`), so `sdk/*/bin` never matches and line 115 aborts on
a fully provisioned machine:

```
$ zsh -c 'for d in tmp/globtest/sdk/*/*/bin tmp/globtest/sdk/*/bin; do echo "MATCH $d"; done'
zsh:1: no matches found: tmp/globtest/sdk/*/bin
```

(the versioned dir exists and would have matched the first pattern; nothing is
printed because the failure precedes the loop body.)

So on zsh the file never completes: line 92 kills it pre-setup, line 115 kills it
post-setup. `[ -d "$_nros_sdk" ]` at line 114 does not help — the directory
exists, only the inner glob is empty.

## Class

Two sites, one root cause — the fix must cover both, not just the reported line
92 (CLAUDE.md "fix the CLASS" rule):

| line | glob |
|---|---|
| `activate.sh:92`  | `"${NROS_HOME:-$HOME/.nros}"/sdk/play_launch_parser/*/bin` |
| `activate.sh:115` | `"$_nros_sdk"/*/*/bin "$_nros_sdk"/*/bin` |

`grep -rn 'for .* in .*\*.*; do' activate.sh scripts/sdk-env.sh scripts/bootstrap.sh`
finds exactly these two. No `setopt`/`null_glob`/`nomatch` guard exists anywhere in
the sourced files.

Note the `[ -x … ] || continue` idiom does **not** fix this: in zsh the failure
happens during word expansion, before the loop body is ever entered. A guard
inside the loop is unreachable.

## Direction

One shared helper used by both sites, rather than a second spelling at line 92
(the `#282 → #326` antipattern). Options, cheapest first:

1. **Expand through a subshell that cannot fail the parent** —
   `_nros_glob_dirs() { ls -d "$@" 2>/dev/null; }` and drive the loop with
   `while IFS= read -r d`. POSIX, no shell-option juggling, works identically in
   bash/zsh/dash.
2. **Guard the option** — `setopt local_options null_glob` when `$ZSH_VERSION` is
   set. Shell-specific and has to be repeated per site; only attractive if the
   `ls` subshell cost matters, which it does not at ~2 invocations per shell.

Whichever lands, add a regression check that *sources* `activate.sh` under zsh
with an empty `NROS_HOME` and asserts the file ran to completion (e.g. that
`sdk-env.sh`'s exports are present), and run it for bash + zsh + fish. Today
nothing sources this file under a non-bash shell in CI — which is why a
phase-218 SSoT file has been broken for zsh users without a red anywhere.

## Secondary finding — `activate.fish` drifted from `activate.sh`

Same file family, worth folding into the same fix: `activate.fish:56` tests only
the **unversioned** `~/.nros/sdk/play_launch_parser/bin`. The phase-327 W3
versioned-store fallback (the `sdk/<tool>/<version>/bin` loop added at
`activate.sh:88–98`) was never mirrored, so a fish user who runs
`nros setup --tool play_launch_parser` — the remedy `just doctor` prints — gets a
parser that fish never puts on PATH. The two files are hand-mirrored by
convention (`activate.sh:9-10`), with nothing checking that they agree.
