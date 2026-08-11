---
id: 516
title: "Every regex reader of `package.xml` treated COMMENTED-OUT elements as
  real declarations"
status: resolved
type: bug
area: build-system
related: [phase-348, issue-0196]
---

## The defect

cmake has no XML parser, so every `package.xml` reader in the tree matched
regexes against raw file text. A regex cannot tell an element from the same
element quoted inside a comment, so this:

```xml
<export>
  <!-- <nano_ros deploy="native" board="native" rmw="zenoh"/> -->
</export>
```

read as a live consumption export selecting zenoh. Seven sites, all with the
same shape:

| file | reads | what a comment could fake |
| --- | --- | --- |
| `NanoRosPackageXml.cmake` | `<nano_ros …/>` | deploy / board / **rmw selection** |
| `NanoRosVerbs.cmake` | `<depend>` presence | "this package has interfaces" |
| `NanoRosGenerateInterfaces.cmake` ×2 | `<name>`, dep tags | package identity, dep set |
| `_NrosFindRosMsgPackage.cmake` ×3 | `<name>`, dep tags | which package a Find-stub resolves to |

The `<depend>` ones are the likeliest to fire in the wild: commenting a
dependency in and out is routine ROS practice, and `NanoRosVerbs.cmake` used
mere *presence* of a `<depend>` tag to decide whether to run interface codegen.
A package whose deps were all commented out still triggered it.

## How it surfaced

phase-348 W1 added the first provider `package.xml`. It explains the difference
between the provision and consumption exports in a comment, quoting the other
tag:

```xml
<!-- Provision, NOT consumption. `<nano_ros rmw="zenoh"/>` in a leaf … -->
```

`nano_ros_read_package_export()` then reported that file — which consumes
nothing — as consuming `rmw=zenoh`. **The file was correct; the reader was
wrong.** The bug predates phase-348 by a long way; documenting a tag is just
the first thing anyone had done that tripped it.

Worth recording that the initial diagnosis was wrong: the regex is
`<nano_ros[ \t\r\n]+`, and `<nano_ros_provides` has an underscore where the
whitespace must be, so the two tags genuinely cannot be confused — that
reasoning was sound and irrelevant. The match came from the comment, not the
sibling tag. A near-miss verified by argument instead of by running it would
have been recorded as safe.

## Fix

One shared helper, `nros_read_package_xml_body(<path> <out_var>)` in
`NanoRosPackageXml.cmake` — read, then strip comments — and all seven sites
converted to it, rather than a strip at the site that showed the symptom.

The pattern is `<!--([^-]|-[^-])*-->`, not `<!--.*-->`: cmake regexes are
greedy and have no lazy quantifier, so the naive spelling eats every element
*between* two comments. That failure mode is covered by a test, because it
would be a silent content-deletion rather than an error.

`scripts/check-rmw-descriptors.py` (S5) got the same strip — it had inherited
the bug at birth, an hour old. `PackageXml::parse` in the CLI was never
affected: `quick_xml` reports comments as their own event. A test pins that
anyway, so a future text-scanning fast path cannot regress it.

## Why it went unnoticed

The class is invisible until someone writes a comment that quotes a tag, and
until phase-348 no `package.xml` in the tree did. The readers were all correct
on every input they had ever been given — a gate over the existing corpus would
have been green, which is the same shape as issue 0196: a probe whose inputs
never included the case that breaks it.
