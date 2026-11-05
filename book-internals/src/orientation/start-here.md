# Start Here

This is **nano-ros Internals**, the book for people who change nano-ros
itself. If you want to *use* nano-ros (write nodes, bring up a board, add a
board or an RMW backend from outside the tree), read the
[nano-ros Book](../../start/welcome.html) instead. Everything there works
from an installed release, while everything here assumes a checkout.

## The two books and the records

| You want… | Read |
| --- | --- |
| To use nano-ros, or to port it to a board, platform, transport or RMW | the [nano-ros Book](../../start/welcome.html) |
| How the tree is built, tested, generated and released | this book |
| Why a decision was made | an RFC in [`docs/design/`](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/README.md) |
| What is being worked on, and its acceptance | a phase doc in [`docs/roadmap/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/docs/roadmap) |
| A known bug, limitation or piece of tech debt | an issue in [`docs/issues/`](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/README.md) |

The three numbered series are **records**. This book is **narrative**: it
explains how the parts work and links to the record that decided each one,
without copying it.

The books are split by reader, not by topic
([RFC-0123](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0123-two-books-by-audience.md)).
When a subject has a user side and a contributor side, each book has its own
page and the two link to each other. For example, the Book's ROS 2 interop page
says what to do, and [RMW Internals → Zenoh protocol](../rmw/zenoh-protocol.md)
says how it works.

## First hour

1. Set up a checkout: [Contributing](contributing.md).
2. Learn the shape of the system: [Architecture](architecture.md), then the
   [repo map](repo-map.md).
3. Learn the verbs. `just` with no arguments lists about eight, grouped by
   when you need them, and `just check <name>` runs one gate.
   [Build Commands](../dev-env/build-commands.md) has the full list.
4. Before you open a pull request, run `just check fast`.

## Where a new fact goes

When you learn something that will still be true later, file it where the
next person will look:

- a design decision goes in an RFC;
- planned work goes in a phase doc;
- a bug or limitation goes in an issue;
- how something works, for contributors, goes on a page of this book;
- how to use something goes on a page of the nano-ros Book.

`CLAUDE.md` and `AGENTS.md` at the repo root are routers for coding agents.
They point here and into `docs/`, and they deliberately hold no design
detail.
