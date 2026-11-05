# Writing the Book

This page is for anyone who writes or reorganizes a page of the nano-ros Book,
the user book under `book/src/`. It is the charter of
[RFC-0124](../../../docs/design/0124-book-user-guide-organization.md) put as a checklist, followed by one template for each kind of page. Read the
checklist before you open a template, and check your page against it before
you ask for review.

`check-book-audience` reads the gated rules. It reports them today (phase-537
W0) and refuses them once phase-537 W13 lands:

```sh
python3 scripts/check-book-audience.py --charter
```

## The checklist

### Who and why

1. **Three purposes, stated budgets.** Every page serves at least one of
   education, field solutions and industrial, within its budget: a 15-minute
   install and labs under two hours; 30 minutes from box to
   `ros2 topic echo`; reproducible, auditable, offline builds (RFC-0116 D1).
2. **Two books by reader.** Application developers and porters read the Book.
   Contributors read this book, Internals. Contributor material in a user page
   moves here.

### What may be in the Book

3. **A board enters the Book only with an emulator twin,** or a track that
   replays its chapter (RFC-0115 D4). QEMU mps2-an385 is the concepts board.
   **Gated:** a board page whose registry rows are neither Verified nor an
   emulator twin of a named board is reported.
4. **Claims follow the registry.** Board facts and status words are generated,
   never typed. A board, feature or integration with no registry row gets no
   user page. **Gated** (rule E refuses a typed status word).
5. **Only the user tier.** No `nros dev`, no checkout steps, no internal ids
   in prose. RFC numbers appear only in a closing "Design background".
   **Gated** (rules D and `check-user-surface-ids` refuse).

### How it is organized

6. **Sections follow what the reader is doing:** Start here, Get started on
   your board, Tutorial, User guide, Background, Reference, Extending,
   Releases. RFC-0124 Appendix A names every page and its sources. A page not
   in Appendix A needs an RFC amendment first.
7. **One topic, one page per type.** Tutorial, how-to, explanation and
   reference are separate pages that link to each other. A topic has at most
   one page of each type. **Gated:** the declared type must fit the section,
   and two pages may not declare the same type and topic.
8. **Starters before depth.** Every admitted board has a starter that needs
   only Install, twin-and-hardware boards first.
9. **Reference is generated from its owner:** the contract format, the CLI,
   the APIs, `system.toml`, boards. A hand-written table that copies a
   generated one is deleted.

### How it is written

10. **Procedure first.** A task page is steps. Each step is a command or an
    edit, followed by its expected output. The page ends with a complete
    working example.
11. **Concepts at the point of need.** Explain a concept only if it is
    uncommon outside nano-ros and central inside it: the bringup, the image,
    the contract, static pools, the execution tiers. Start from the example in
    front of the reader, not from the model. For what a ROS 2 user knows, link
    to the ROS 2 documentation.
12. **The simplest working path first.** Serial before ethernet, with WiFi
    stated as deferred. The first chapter names no launch file, image or
    entry. One run verb, `nros run`, with `nros flash --monitor` on hardware.
13. **Say plainly where nano-ros differs from ROS 2.** Never imply a symmetry
    that is not there.
14. **Every runnable step is probed** on its row's evidence. A step that
    cannot run yet carries an `awaits` line on its track and fails loudly.
    Nothing is skipped. **Gated:** a shell fence with no `probe=` is reported.
15. **Budgets.** **Gated:**

    | Type | Words of prose |
    | --- | --- |
    | how-to | 300–1,500 |
    | tutorial chapter | 1,500–3,000 |
    | explanation | 800–2,500 |
    | reference | no limit, but tables and lookups only |

    A page under 300 words is merged into its parent. Rust, C and C++
    variants of one step are tabs on one page, not parallel pages.

### Voice

Follow the
[Google developer documentation style guide](https://developers.google.com/style):
second person, present tense, active voice, sentence-case headings, and no
"simply", "just" or "easy". Name a thing once, then use that name throughout.

## Declare the type and level

Every hand-written user page carries one declaration on the line after its
title. Generated pages are reference pages and carry none.

```markdown
# Configure zenoh

<!-- type: how-to · level: 2 -->
```

Add `· topic: <slug>` when a topic has pages of several types, so the gate
can tell that each type appears once:

```markdown
<!-- type: tutorial · level: 2 · topic: contract -->
```

### Topic levels

A page may assume only topics at its own level or below. It lists them on a
"needs first" line, and the gate reports a link to a higher level.

| Level | Topics | Where |
| --- | --- | --- |
| 1. First contact | install; a board starter; run a talker; what you built; add a node; pick and configure the RMW | Start, starters, Tutorial 1–4 |
| 2. Building real applications | your own message; services and actions; parameters and launch files; the contract and sizing; another target; ROS 2 interop and QoS; a serial link; logging; workspace layout; mixing languages | Tutorial 5–10, User guide |
| 3. Production and tuning | configuring an image; build profiles; offline and vendored builds; deploying; static memory; scheduling and timing; throughput; bridges; porting a ROS 2 node | User guide, Background |
| 4. Extending nano-ros | a board, platform or transport; a custom RMW backend; runner and command packages; API design | Extending, then Internals |

A reference page carries the level of the most advanced topic it describes.
Anyone may look it up.

## Languages in tabs

When a step has a Rust, a C and a C++ version, write the versions as one tab
group. The rendered book shows them as tabs, and the language a reader picks
switches every group on the page and stays picked on the next page.

````markdown
<!-- tabs -->

#### Rust

```rust
executor.spin_once(10)?;
```

#### C

```c
rclc_executor_spin_some(&executor, 10);
```

#### C++

```cpp
executor.spin_some(10);
```

<!-- /tabs -->
````

It renders as:

<!-- tabs -->

#### Rust

```rust
executor.spin_once(10)?;
```

#### C

```c
rclc_executor_spin_some(&executor, 10);
```

#### C++

```cpp
executor.spin_some(10);
```

<!-- /tabs -->

The rules:

- `<!-- tabs -->` and `<!-- /tabs -->` each stand alone on a line, at column
  0, so a group cannot sit inside a list or a quote.
- The first line after `<!-- tabs -->` (blank lines aside) is a heading whose
  text is `Rust`, `C` or `C++`. Every heading at that level inside the group
  starts the next tab. Use one level below the section you are in.
- A tab holds any markdown: prose, several fences, a list. Deeper headings
  are content. A group has at least two tabs and each language once.
- Show only the languages the step has. Rust first, then C, then C++.

On GitHub, which does not run the preprocessor, a group reads as one
sub-heading per language. The tab headings do not appear in the rendered page,
so do not link to them.

A probe step inside a tab belongs to that tab's language. Give each
language's version of a step the same `probe=NN`, and set `lang` (`rust`, `c`
or `cpp`) on the `[[probe]]` row of each track that replays the page, in
`packages/boards/board-support.toml`. The extractor keeps that language's
blocks and every untabbed one, and it refuses a tabbed step on a track that
names no language. The charter counts a group's prose once, as its longest
tab.

`scripts/mdbook-lang-tabs.py` renders the groups and `scripts/lib/book_tabs.py`
parses them. `check-book-audience` refuses a malformed group.

## Templates

Each template gives the headings in order. Replace the comments. Keep the
headings a reader scans for, even when a section is one line.

### Tutorial chapter

One chapter of the one Tutorial project, run on a Verified row, small enough
for a two-hour lab.

```markdown
# <What the reader does, as a verb phrase>

<!-- type: tutorial · level: <1|2> -->

**Needs first:** [<previous chapter>](<link>)

<!-- One paragraph: what the reader has at the end, stated as output they will see. -->

## <Step 1, a verb phrase>

<!-- The command or edit. Then the output they should see. -->

## <Step n>

## What just happened

<!-- The ONE nano-ros concept this chapter introduces, explained from the
     example the reader just ran. Link to Background for the full model. -->

## Next

<!-- The next chapter, and the User guide pages that go deeper. -->
```

### How-to

One task, for a reader who already knows what they want.

```markdown
# <Task, as a verb phrase: "Use a serial link">

<!-- type: how-to · level: <2|3|4> -->

**Needs first:** [<page>](<link>)

<!-- One sentence: when you would do this. -->

## Before you start

<!-- What must already be true: board, packages, a working project. -->

## Steps

1. <!-- A command or edit. -->

   <!-- Its expected output. -->

## Example

<!-- One complete, working example the reader can copy whole. -->

## If it doesn't work

<!-- The two or three failures a reader actually meets, each with its fix. -->

## See also

<!-- The reference page for the options; the Background page for the why. -->
```

### Board starter

The Tutorial's chapter 1 done on one board, needing only Install. About
1,500 words. The status header and the list of starters are generated from
the registry; do not type them.

```markdown
# Get started on <board>

<!-- type: how-to · level: 1 -->

**Needs first:** [Install](<link>)

## What you need

## Provision

## Scaffold the project

## Run it in the <emulator or twin>

## Flash the board

<!-- Only when the row names `hardware`. -->

## See it in ROS 2

## If it doesn't work

## Next
```

### Platform page

What the platform's starter leaves out: configuration, board notes, deeper
tasks. Needs only Tutorial 1–2 and the starter. It sits in the User guide
group its row's generated status word names.

```markdown
# <Platform>

<!-- type: how-to · level: <2|3> -->

**Needs first:** [Get started on <board>](<link>)

## Configure the platform

## Board notes

## <Deeper task, one heading each>

## Reference

<!-- Links to the generated board pages. -->
```

### Explanation

A Background page: the nano-ros "why" a reader needs before production work.

```markdown
# <The thing explained, as a noun phrase>

<!-- type: explanation · level: 3 -->

<!-- Open with a concrete example the reader has already run or will run:
     a build, a log line, a measured number. Then explain the model behind
     it. No steps. -->

## <Part of the model>

## Where nano-ros differs from ROS 2

<!-- Stated plainly, when it differs. -->

## Design background

<!-- The only place an RFC number appears. -->
```

### Reference

Lookup only: tables, keys, options. If the page's content has an owner (the
CLI's help, a schema, a generator), generate the page from it instead.

```markdown
# <Thing> reference

<!-- type: reference · level: <level of its most advanced topic> -->

<!-- One sentence: what this lists and where it is generated from. -->

| Name | Type | Default | Meaning |
| --- | --- | --- | --- |
```

## Before you ask for review

State in the PR description, for every page you add or rewrite:

- its type, level and section in RFC-0124 Appendix A;
- the principles you checked it against, and any you could not meet yet,
  with the reason (for example, a step waiting on a CLI change, with its
  `awaits` line);
- the output of `python3 scripts/check-book-audience.py --charter` for that
  page.

Moves go through `book/page-map.toml` and `scripts/book-move.py`, so every
moved page keeps a redirect. Land the work in section-sized PRs, rebased onto
main rather than merging main in.
