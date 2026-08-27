---
id: 844
title: "`check-no-tracked-file-find` scanned only `scripts/`, so a `grep -r` over
  9.2 GB of gitignored Zephyr SDK sat unseen in a test script and stalled the
  fixture build for 37 minutes"
status: resolved
resolved_in: phase-392
type: tech-debt
area: testing
related: [issue-0196, issue-0721, issue-0726]
---

## What happened

`just build-test-fixtures lane=native` sat for **37 minutes** with no output and
no compiler running. It was not hung. `core_only_predicate.sh` was running:

```sh
consumed="$(grep -rhoE '[a-z0-9-]+ +[a-z]+ +--core-only' just/ scripts/ 2>/dev/null | ...)"
```

`scripts/` is 9.2 GB on a provisioned host, because the Zephyr SDK lives there:

| path | size | tracked? |
| --- | ---: | --- |
| `scripts/zephyr/sdk/` | 7.8 GB | gitignored |
| `scripts/zephyr/downloads/` | 1.4 GB | gitignored |
| everything else under `scripts/` | ~1.2 MB | tracked |

So the recursive grep read the entire toolchain — compilers, sysroots, tarballs
— looking for a CLI flag that only ever appears in tracked shell and `just`
source. The index lookup returns the same answer:

```
git grep -hoE '...' -- just/ scripts/   →  linux, in 0.33s
grep -r  ...        just/ scripts/      →  37+ minutes, killed
```

Fixed in `core_only_predicate.sh`. An untracked match could only be inside the
SDK, which is not a caller, so scoping to the index loses nothing.

## Correction — my first diagnosis was wrong

I filed this as *"the gate does not detect `grep -r`"*. It does, and has for a
long time: `check-no-tracked-file-find.sh` line 108 carries

```python
g = re.search(r"(?<!git )\bgrep\s+-[a-zA-Z]*[rR]", head)
```

and reports it with the `git grep -- <pathspec>` remedy. I wrote a second arm
for a check that already existed, and only noticed because both arms flagged the
same three lines with different wording. The duplicate is gone.

The real defect is narrower and duller, which is why it survived: **the gate
scanned three paths.**

```python
FILES = git ls-files scripts just justfile
```

`core_only_predicate.sh` lives in `packages/testing/nros-tests/tests/`, so the
gate never opened it — along with the other 45 tracked shell scripts outside
`scripts/`. A gate stating a repo-wide rule while reading one directory is the
same rule/pattern mismatch its own header records twice (issues 0721, 0726),
one level further out: there the PATTERN was too narrow, here the FILE SET was.

## Resolution

- Scope widened to every tracked `*.sh` (minus `third-party/`) plus `just/` and
  `justfile`. Verified against the offender: reintroducing the original line
  now fails the gate at `core_only_predicate.sh:70`.
- The three live `grep -r` sites the widened scan exposed are converted to
  `git grep -- <pathspec>`. All three were rooted at tracked `src/` dirs, i.e.
  harmless today and none of them had to be:
  `build_root_derivation.sh:501`, `size_probe_verify.sh:52` and `:57`.
- One `find` site surfaced too, `check_parser_failures.sh:56`, and is a genuine
  exemption rather than a fix: its root derives from `$ROS_SHARE`, a ROS install
  prefix outside the repo, so the index cannot see those `.msg` files at all.
  `$MSG_DIR` joins the `NO_INDEX` list the gate already keeps for out-of-repo
  roots.

## Note for anyone timing CI on this host

The stall is worst here because CARLA/Autoware were saturating the disk at the
same time. On an idle host the same grep would still read 9.2 GB; it would just
be less obvious about it.
