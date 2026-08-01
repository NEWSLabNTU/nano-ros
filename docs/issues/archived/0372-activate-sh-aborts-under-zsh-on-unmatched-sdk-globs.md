---
id: 372
title: "`activate.sh` aborts mid-file under zsh — unmatched SDK globs are fatal, so the second half of the activation never runs"
status: resolved
type: bug
area: build
related: [rfc-0014, issue-0204, issue-0368, issue-0375, phase-218, phase-327]
resolved_in: "3de28c939"
---

# `activate.sh` aborts under zsh on unmatched SDK globs

`activate.sh` is `source`d, and zsh's default `nomatch` makes an unmatched glob
a **fatal** error, so activation died mid-file and silently dropped every export
below the failure — the SDK-store PATH loop that wires `zenohd`, pinned
ninja/make, `.env`, `scripts/sdk-env.sh`. The file's header claimed
"POSIX shell (bash / zsh)" and "the script never errors"; both were false on
zsh. Found walking the book's install flow on Arch Linux with zsh as `$SHELL`.

Three sites, one class:

| site | glob | fires when |
|---|---|---|
| `activate.sh:92` | `…/sdk/play_launch_parser/*/bin` | empty store (fresh machine) |
| `activate.sh:115` | `"$sdk"/*/*/bin "$sdk"/*/bin` | provisioned store — zsh applies `nomatch` **per word**, and `sdk/*/bin` never matches the versioned layout `nros setup` writes |
| `activate.sh:121` | `ls "$dir"/*-gcc` (loop body) | any store dir without a cross-gcc |

So the file never ran to completion under zsh in **any** state.

## Resolution

`3de28c939`. Two helpers built on `find` (`_nros_bin_dirs`,
`_nros_dir_has_gcc`) — `find` reports nothing instead of failing — with all
three call sites routed through them, and results read via `while read` +
heredoc rather than a pipe so `export PATH` lands in the sourced shell. The
invariant for this file is now: **no glob reaches the shell's word expansion**.
`[ -x "$d" ] || continue` inside the loop is not an alternative — zsh fails
during expansion, before the body runs.

Gated by `check-activate-shells` (in `check-fast`): sources `activate.sh` and
`activate.fish` under bash + zsh + fish against an empty store and a
versioned-only store, asserting each reaches its final lines; absent
interpreters skip loudly. Reverting either glob site was confirmed to turn it
red — site 1 on the empty-store case, site 2 on the versioned-store case. The
sentinel is *completion*, not exit status: a zsh `nomatch` abort ends the
sourced file while the outer shell still exits 0, which is why nothing noticed
for so long.

Secondary finding, fixed in the same commit: `activate.fish` never received
phase-327 W3's versioned-store fallback (`sdk/<tool>/<version>/bin`), so a fish
user's `nros setup --tool play_launch_parser` installed a parser fish could not
find. fish is not affected by the glob class itself — an unmatched wildcard
there skips the loop and continues.
