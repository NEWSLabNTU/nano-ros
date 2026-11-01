# Adoption probes

A book probe (`just probe <platform>`) replays a chapter mechanically and
checks that its commands still print what the chapter says. An adoption probe
replays a **purpose**: someone who has only the book, a pristine host and a
budget tries to get to the purpose's first result, and every place they stop
becomes an issue. RFC-0116 D9 defines the method; phase-523 W3 runs it.

| Purpose | Brief | Run by | Budget |
| --- | --- | --- | --- |
| Industrial | [industrial.md](industrial.md) | a Claude session given the brief | reproducible, auditable, offline-capable |
| Field solutions | [field.md](field.md) | a real technician or student with a board | 30 minutes, unopened box to `ros2 topic echo` |
| Education | [education.md](education.md) | a Claude session given the brief | 15-minute install, a lab inside two hours, the same output on every seat |

The field probe is never run by a model. Its budget is about what a person does
with a box, and a model's reading of a chapter is not evidence of that.

## What counts as a stop

Anything that makes the reader leave the book or wait past the budget:

- a command that does not exist, or fails, as the chapter writes it;
- a word the chapter uses before defining it (`bringup`, `image`, `entry`, `west`
  and the like, unless the page explains it);
- an error whose message does not name the next command to run;
- a step the chapter skips (an install it assumes, a directory it assumes you
  are in);
- a wait longer than the purpose's budget allows, with the time measured.

Record each stop in the run log as you hit it: the page and section, the exact
command, the decisive line of output, and the time since the start. Then go on
if you can find a way, and say what you did. A stop you worked around is still
a stop.

## Filing

Each stop is one issue. Reserve the id with `just issue-new <slug>` and add a
`purpose:` line to its frontmatter (`industrial`, `field` or `education`):

```yaml
---
id: NNNN
title: "first-node.md: `nros build` fails before `nros setup` was run, and the error does not say so"
status: open
type: bug
area: book, cli
purpose: field
related: [rfc-0116, phase-523]
---
```

`just issues --purpose field` lists one purpose's open stops, and the
generated `docs/issues/open.md` groups them under "Adoption-probe stops by
purpose".

## Rounds

A purpose is probed twice. Round one finds the stops; the fixes land; round two
counts what remains. Each round's count per purpose goes on phase-523's status
line. If round two for a purpose finds more stops than round one closed, that
purpose's chapters are rewritten before another board is added.

## Running a Claude-session probe

Give the session the brief and nothing else from this repository: no
`CLAUDE.md`, no `AGENTS.md`, no design documents, no source tree. It works in a
pristine container (the brief names the image), reads the published book
(<https://newslabntu.github.io/nano-ros-book/>) or a rendered copy of
`book/src/`, and writes its run log and the list of stops as its report. A
session that has read this repository's internals is not a newcomer; do not
reuse one.
