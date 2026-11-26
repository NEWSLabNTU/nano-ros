//! The diagnostic codes. This file is the single source of truth: every view
//! (`codes.lock`, the book reference, `nros explain`) is generated from it.
//!
//! A new code's number is reserved on origin with `just code-new <Name>`
//! before it is written here; reading "the next free number" is the race that
//! collided issue ids. Rows are in number order. A number is never reused or
//! renumbered: retire a code with `NNNN Retired(Name) => successor,`.
//!
//! NROS-0001 to NROS-0006 (the founding range, [`crate::FOUNDING_MAX`]) were
//! allocated in the founding commit, before claims existed; every later code
//! is claimed on origin first, and is not allocated if the claim fails.

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

    /// A `system.toml` env row is spelled with a knob's retired name. The
    /// knob registry (`nros-zephyr-build/knobs.toml`) holds both spellings,
    /// so the rename is mechanical and `nros fix` applies it. History:
    /// phase-484 W8 (the refusal), phase-578 W4 (its location and fix).
    0011 RetiredKnobSpelling {
        title: "a knob is spelled with its retired name",
        surfaces: [Cli],
        remedy: "rename the key to the knob's current spelling, which the refusal names; \
                 `nros fix` does it",
    }

    /// An `[image.<id>] env` (or `[image_defaults] env`) row is keyed by a
    /// knob the registry holds. A knob is stated in the image's knobs table,
    /// where its type and range are checked; the env row was deprecated in
    /// release 0.6.0 and is refused since. History: phase-484 W8 (the
    /// refusal), phase-578 W4 (its location and fix).
    0012 ImageEnvNamesKnob {
        title: "an image's env row sets a registry knob",
        surfaces: [Cli],
        remedy: "move the row to the image's `[image.<id>.knobs.<group>]` table, which the \
                 refusal names; `nros fix` does it",
    }

    /// An image's `board` is `native` or `posix`. Both were deprecated
    /// aliases of the host board, which has one name, `linux`. History:
    /// phase-500 W5 (the refusal), phase-578 W4 (its location and fix).
    0013 RetiredBoardKey {
        title: "an image names a retired host board key",
        surfaces: [Cli],
        remedy: "write `board = \"linux\"`; `nros fix` does it",
    }

    /// An image's `framework` is one its board cannot boot, or one the board
    /// decides itself. The board definition is the authority: its
    /// `frameworks` list, or the entry shape its platform implies. History:
    /// RFC-0111 (the field), phase-578 W4 (both locations).
    0014 ImageFrameworkRefused {
        title: "an image states a framework its board does not boot",
        surfaces: [Cli],
        remedy: "choose a framework from the board's `frameworks` list, or remove the line so \
                 the board's default is used",
    }

    /// The pre-build compatibility check found reasons an image cannot build
    /// for its board; each reason below it carries its own code. History:
    /// RFC-0135 D8, phase-577 W6.
    0015 ImageBoardIncompatible {
        title: "an image cannot build for its board",
        surfaces: [Cli],
        remedy: "fix each reason listed, or choose a board that has what the image needs",
    }

    /// The board and the languages of the image's packages name a
    /// combination nano-ros has no build road for (a C or C++ package on a
    /// board whose road compiles Rust only, and the like). History: RFC-0065
    /// D3; collected by RFC-0135 D8. The framework check is NROS-0014.
    0016 BoardNoBuildRoad {
        title: "no build road takes this image's languages to this board",
        surfaces: [Cli],
        remedy: "choose a board whose build road compiles every language the image uses, or \
                 move the image's C or C++ code into a package for another image",
    }

    /// The image's RMW backend needs a board capability (`[rmw.needs]`) that
    /// the board's `[board.capabilities]` does not state. History: RFC-0135
    /// D8, phase-577 W6.
    0017 RmwNeedsCapability {
        title: "the image's RMW needs a capability the board lacks",
        surfaces: [Cli],
        remedy: "choose an RMW this board can run (`nros rmw list`), or a board with the \
                 capability",
    }

    /// The image's network or link cannot be carried on this board: a link
    /// the board's `[board.net] links` does not list, an address outside its
    /// network, or an endpoint the RMW has no arm for on this platform.
    /// History: RFC-0109 D3, RFC-0103 D10; collected by RFC-0135 D8.
    0018 LinkNotOnBoard {
        title: "the image's network or link is not one the board carries",
        surfaces: [Cli],
        remedy: "change the image's `locator` or network to one the board carries (`nros \
                 board net <board>`), or choose a board that has the link",
    }

    /// A toolchain or tool the board builds with is not installed. History:
    /// RFC-0065 D2 (the toolchain preflight); collected by RFC-0135 D8.
    0019 BoardToolMissing {
        title: "a tool the board builds with is not installed",
        surfaces: [Cli],
        remedy: "run the `run:` line the reason prints; `nros setup <board>` installs \
                 everything the board needs",
    }

    /// The board package names an environment variable for its vendor SDK
    /// root (`[board.sdk] root_env`), and it is not set or names no
    /// directory. History: RFC-0135 D8, phase-577 W6.
    0020 BoardSdkRootUnset {
        title: "the board's vendor SDK root is not set",
        surfaces: [Cli],
        remedy: "export the variable the reason names, set to the vendor SDK's directory",
    }
}
