//! The diagnostic codes. This file is the single source of truth: every view
//! (`codes.lock`, the book reference, `nros explain`) is generated from it.
//!
//! A new code's number is reserved on origin with `just code-new <Name>`
//! before it is written here; reading "the next free number" is the race that
//! collided issue ids. Rows are in number order. A number is never reused or
//! renumbered: retire a code with `NNNN Retired(Name) => successor,`.
//!
//! NROS-0001 to NROS-0006 were allocated in the founding commit, before the
//! reservation recipe existed; every later code is reserved.

diagnostics! {
    /// `nros configure` (and the build and run verbs that configure first)
    /// has messages to generate, `AMENT_PREFIX_PATH` is unset, and no earlier
    /// configure of the project recorded a prefix. History: RFC-0120 D2;
    /// issue 2279 (the remedy names the verb the reader ran).
    0001 AmentPrefixUnset {
        title: "ROS 2 is not sourced, and the project has messages to generate",
        surfaces: [Cli],
        remedy: "source ROS 2 in this shell (`source /opt/ros/<distro>/setup.bash`, or your \
                 overlay's `install/setup.bash`) and run the command again; later shells need \
                 not source it",
    }

    /// An image's `board` is claimed by no board descriptor's `names`.
    /// History: issue 0606 (the catalog's one resolution rule).
    0002 BoardUnknown {
        title: "an image names a board that no board descriptor claims",
        surfaces: [Cli],
        remedy: "check the spelling of the image's `board`; `nros board list` names every \
                 board this SDK has",
    }

    /// A workspace, or a project, declares no `[image.<id>]` at all.
    0003 NoImageDeclared {
        title: "the workspace declares no image",
        surfaces: [Cli],
        remedy: "add `[image.<id>] board = \"<board>\"` to the bringup package's system.toml",
    }

    /// An `[image.<id>]` table has no `board` key.
    0004 ImageNoBoard {
        title: "an image names no board",
        surfaces: [Cli],
        remedy: "add `board = \"<board>\"` to the `[image.<id>]` table in system.toml",
    }

    /// A board names a platform whose `inherits` chain reaches no built-in
    /// family (or which nothing answers to, or whose chain loops). History:
    /// issue 2310, user ruling of 2026-11-12 (option 3).
    0005 PlatformNoFamily {
        title: "a board's platform reaches no built-in family",
        surfaces: [Cli],
        remedy: "add `inherits = \"<family>\"` to the platform's nros-platform.toml",
    }

    /// `nros explain NROS-NNNN` names a code that was never allocated.
    0006 UnknownDiagCode {
        title: "no diagnostic has this code",
        surfaces: [Cli],
        remedy: "check the code against the refusal that printed it",
    }
}
