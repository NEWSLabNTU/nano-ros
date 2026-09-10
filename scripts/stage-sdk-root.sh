#!/bin/sh
# Stage the nano-ros SDK root into a release prefix — phase-447 A1, RFC-0099 D2.
#
# The SDK root is what a BUILD reads: the cmake modules a workspace includes,
# the board and platform descriptors, and the runtime crates a user's project
# compiles against. Until phase-447 a release staged `bin/nros`, the index and
# `install.sh`, so the journey `install.sh` -> `nros setup <board>` -> `nros new`
# -> `nros build` dead-ended at the last step with "no nano-ros checkout found"
# (RFC-0099 D1). This is the missing member.
#
# It goes INSIDE the toolchain asset, at `<prefix>/share/nano-ros`, because
# RFC-0097 D6 already decided codegen and the runtime are ONE unit — a
# separately-versioned SDK would split exactly that unit and reopen the
# acceptance-range question D6 closed.
#
# WHY `git archive` AND NOT `cp -r`
#
# The working tree carries ~20 G of build output; `git ls-files` is this repo's
# rule for enumerating the tree, and `git archive HEAD` is that rule with the
# copy attached. It also pins the payload to the COMMIT — the same commit
# `share/nros/manifest.toml` records as `nano_ros` — rather than to whatever a
# runner's working tree happens to hold. Submodule gitlinks are skipped, which
# is correct: `packages/cli/third-party/play_launch` is a build-time dep of the
# CLI, not of a user's project.
#
# WHAT IS STAGED, AND WHY EACH PART IS NOT OPTIONAL
#
# Measured against what a build actually reads, not assumed. RFC-0099 D2 named
# `{cmake,config,packages}`; that set is REAL but not sufficient, and the four
# additions below each have a reader:
#
#   cmake/              every module a workspace includes, the platform and
#                       board overlays, and the cross-toolchain files a board
#                       descriptor names.
#   config/            `config/rust-targets.txt` (stage-3 preflight: `rustup
#                       target add` vs `-Zbuild-std`) and the platform
#                       descriptors `nros-platform-config` resolves.
#   packages/          `BoardCatalog::load_with_packages` reads
#                       `packages/boards/**/nros-board.toml`; a generated entry
#                       path-deps `packages/api/nros`,
#                       `packages/platform/nros-platform` and its board crate;
#                       `packages/interfaces/` is where `std_msgs` and friends
#                       resolve from. Staged WHOLE apart from `packages/cli`,
#                       because 58 of its subdirectories are members of the root
#                       `Cargo.toml` — including `packages/testing` and
#                       `packages/verification`, which no build reads but whose
#                       absence makes the workspace manifest unloadable, and
#                       therefore every Rust user project unbuildable.
#   packages/cli       EXCLUDED, then four paths carved back. The directory is
#                       75.9 MB tracked and mostly ships as a binary, so the
#                       exclusion is worth having — but "nothing in a user's
#                       graph reaches it" was FALSE, and staging is the only
#                       place that could have found out. What is carved back,
#                       each MEASURED by building a scaffolded workspace against
#                       a staged prefix rather than read off a manifest:
#                         nros-entry-lower, nros-pkg-index — `packages/core/
#                           nros-macros` PATH-DEPS both (`../../cli/…`), and
#                           every Rust user project compiles `nros-macros`. Their
#                           absence is not a missing feature: cargo cannot LOAD
#                           the graph, so `_cargo-build_nros_c` and
#                           `_cargo-build_nros_cpp` fail before compiling
#                           anything — i.e. the C++ quick start too, not just
#                           Rust.
#                         Cargo.toml — both carry `version.workspace = true`, so
#                           cargo must find the CLI workspace manifest above them
#                           to parse either one.
#                         interfaces/ — the bundled ROS `.msg` share dirs
#                           `bundled_interfaces_dir` resolves, which the shipped
#                           C++ workspace template needs for its
#                           `<depend>std_msgs</depend>`.
#                       Anything else under `packages/cli` is the CLI's own
#                       source or its test workspaces; a user's graph reaches
#                       none of it, and `test ! -d nros-cli-core` below keeps
#                       that true.
#   Cargo.lock         The root workspace's lock. The SDK root IS a cargo
#                       workspace — corrosion builds `packages/api/nros-c` and
#                       `nros-cpp` out of it — so without the lock the first
#                       build either re-resolves every dependency (a version set
#                       nobody tested, WRITTEN INTO the install prefix) or, under
#                       this repo's project-wide `--locked`, fails with `cannot
#                       create the lock file … because --locked was passed`.
#                       Shipping it is the same promise a lock always makes:
#                       someone else's build resolves what ours did.
#   zephyr/            NOT just the Zephyr lane. `NanoRosSharedCargoDir.cmake`
#                       reads `zephyr/cmake/nros_cargo_build.cmake` as the
#                       AUTHORED knob-name inventory on every C/C++ configure
#                       and FATAL_ERRORs when it is absent — its own comment
#                       says "nano-ros is consumed only as a source distribution
#                       … its absence is a real breakage rather than a reason to
#                       degrade". This line is what keeps that true for a
#                       release.
#   scripts/           `scripts/nuttx/build-nuttx.sh` (NuttX kernel
#                       provisioning, invoked by the board overlay) and
#                       `scripts/check-shared-cargo-dir-used.sh` (build-time
#                       assertion when NROS_SHARED_CARGO_ROOT is set).
#   CMakeLists.txt     `_nros_import_once` does `add_subdirectory("${root}")`,
#                       so the root listfile IS executed by every workspace.
#   nano_rosConfig.cmake
#                      the `find_package(nano_ros)` entry point every generated
#                       cmake root uses.
#   Cargo.toml         REQUIRED by every Rust build: all 58 workspace members
#                       are under `packages/` and inherit `version.workspace`
#                       and `[lints] workspace = true`, so cargo must find this
#                       manifest by walking up from a path-dep'd crate. Without
#                       it a Rust user project fails at manifest parse.
#   nros-sdk-index.toml
#                      read FROM THE SDK ROOT by cmake modules that derive it
#                       from their own location — `NanoRosCorrosion.cmake`
#                       (`…/../nros-sdk-index.toml`, the Corrosion pin) and
#                       `NanoRosCrossToolchain.cmake`
#                       (`…/../../nros-sdk-index.toml`, the toolchain pins).
#                       This is a SECOND copy: `share/nros/nros-sdk-index.toml`
#                       is the one `setup::shipped_index` reads, resolved from
#                       the BINARY. Both are written from the same file here, so
#                       they cannot disagree.
#   examples/qemu-armv7a-nuttx/rust-toolchain.toml
#                      the NuttX nightly-channel SSoT, read as DATA by both
#                       NuttX board overlays. It degrades silently when absent
#                       (an `if(EXISTS)`), and the degradation is `E0463: can't
#                       find crate for core` several minutes into a build.
#
# NOT staged, each on purpose:
#
#   tools/             `cmake/bootstrap.cmake` treats `tools/setup.sh` PLUS the
#                       index as its "am I in a nano-ros source tree?" signal
#                       and returns early when either is missing. Staging the
#                       index without `tools/` is what makes an installed prefix
#                       read as "consumed from an install prefix — nothing to
#                       bootstrap", which is exactly right.
#   third-party/       gitignored; provisioned by `nros setup`, never shipped.
#   docs/ book/ tests/ just/ ci/ .github/ changelog.d/
#                      development surfaces with no build reader.
#   examples/ (rest)   copy-out projects, not inputs to a user's build. Only the
#                       one file above has a reader.
#
# Usage: scripts/stage-sdk-root.sh <prefix>
#   Stages into <prefix>/share/nano-ros and verifies the result.

set -eu

if [ $# -ne 1 ]; then
    echo "usage: $0 <prefix>" >&2
    exit 2
fi
prefix="$1"

# The repo this script lives in, so the caller's cwd does not decide the payload.
repo="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)"

# The subdirectory of the prefix. Kept in step with
# `orchestration::nano_ros_root::SHIPPED_SUBDIR`, which is where the CLI looks.
SHIPPED_SUBDIR="share/nano-ros"
root="$prefix/$SHIPPED_SUBDIR"

# One list, read by the archive below and by `--list`. Every entry is documented
# in the header; a new one belongs there too.
PATHS="cmake
config
packages
zephyr
scripts
CMakeLists.txt
nano_rosConfig.cmake
Cargo.toml
Cargo.lock
examples/qemu-armv7a-nuttx/rust-toolchain.toml
:(exclude)packages/cli"

# The `packages/cli` paths carved back IN. They need their own archive
# invocation: git applies every `:(exclude)` pathspec LAST, so listing them
# beside the exclusion drops them anyway (measured — the first version of this
# script staged nothing under `packages/cli`).
#
# Each is justified in the header. The two crates are reached by a PATH DEP from
# `packages/core/nros-macros`, which every Rust user project compiles and which
# both the C and C++ quick starts reach through `nros-c` / `nros-cpp`; the
# manifest is what their `version.workspace = true` inherits from.
CARVED="packages/cli/interfaces
packages/cli/Cargo.toml
packages/cli/nros-entry-lower
packages/cli/nros-pkg-index"

if [ "$prefix" = "--list" ]; then
    printf '%s\n' "$PATHS" "$CARVED"
    exit 0
fi

mkdir -p "$root"
# shellcheck disable=SC2086
# Word-splitting is intended: these are newline-separated pathspecs and none
# contains whitespace.
git -C "$repo" archive HEAD -- $PATHS | tar -x -C "$root"
# shellcheck disable=SC2086
git -C "$repo" archive HEAD -- $CARVED | tar -x -C "$root"

# The index, from the SDK root's own position. See the header — this is a second
# copy of `share/nros/nros-sdk-index.toml`, written from one source so the two
# cannot drift.
cp "$repo/nros-sdk-index.toml" "$root/nros-sdk-index.toml"

# VERIFY, here rather than in the caller: a staging step that produced an
# incomplete root would otherwise be discovered by a user, several minutes into
# their first build, with an error about a missing include. Each path below is
# an entry point named in the header, so this fails on the thing it is about.
#
# No `exit 1` — `set -e` carries these, which keeps `check-release-manifest`'s
# R4 (every release-blocking `exit 1` must be about `codegen`) meaning what it
# says. Same idiom the workflow's own install probe already uses.
#
# Each assertion SAYS what it is about. A bare `test -f` under `set -e` fails
# with NO OUTPUT, so an incomplete staging read as "the script died" and the
# operator had to bisect this block to learn which path was missing (measured
# while mutation-testing it: two mutants went red in silence).
need() {  # need <flag> <rel-path> <what it is for>
    if [ ! "$1" "$root/$2" ]; then
        echo "nano-ros: the staged SDK root is INCOMPLETE — missing $2" >&2
        echo "  ($3)" >&2
        echo "  The path list is at the top of this script; its header says why each entry is there." >&2
        return 1
    fi
}
refuse() {  # refuse <flag> <rel-path> <why it must not be here>
    if [ "$1" "$root/$2" ]; then
        echo "nano-ros: the staged SDK root carries $2, which it must not." >&2
        echo "  ($3)" >&2
        return 1
    fi
}
need -f packages/core/nros-core/Cargo.toml \
    "the root MARKER — without it nothing recognises this directory as a nano-ros root"
need -f cmake/NanoRosWorkspace.cmake "the workspace modules every user CMakeLists includes"
need -f nano_rosConfig.cmake "the find_package(nano_ros) entry point"
need -f CMakeLists.txt "the listfile _nros_import_once add_subdirectory()s"
need -f Cargo.toml "the workspace manifest every member inherits from"
need -f Cargo.lock "the resolution this release was built on"
need -f zephyr/cmake/nros_cargo_build.cmake \
    "the authored knob inventory NanoRosSharedCargoDir reads on EVERY C/C++ configure"
need -f config/rust-targets.txt "the stage-3 preflight's rustup-target-vs-build-std list"
need -d packages/boards "the board catalog BoardCatalog::load_with_packages reads"
need -d packages/interfaces "the pre-generated msg packages core crates need before codegen runs"
need -d packages/cli/interfaces/std_msgs "the bundled ROS .msg share dirs"
need -f packages/cli/Cargo.toml "the CLI workspace the carved-back crates inherit their version from"
refuse -d packages/cli/nros-cli-core "the CLI's own source ships as a binary"
need -f nros-sdk-index.toml "the Corrosion + cross-toolchain pins, read from the SDK root's own position"

# The carve-back set is DERIVED and re-checked, never trusted. A new path dep
# from a runtime crate into `packages/cli` reads as an ordinary manifest edit and
# would ship a root cargo cannot LOAD — the build fails before compiling
# anything, in every language, and the message names a path inside the asset. So
# every relative `path = "../../cli/<crate>"` in a tracked manifest OUTSIDE
# `packages/cli` must resolve to something staged. Read from HEAD, which is what
# `git archive` above staged. (Fixture templates spell it
# `@NANO_ROS_ROOT@/packages/cli/...`; that is not a cargo path dep and belongs to
# no workspace, so the relative form is the right pattern to key on.)
missing=""
for dep in $(git -C "$repo" grep -h -o -E 'path = "(\.\./)+cli/[A-Za-z0-9_.-]+"' HEAD \
        -- '*Cargo.toml' ':(exclude)packages/cli' | sed -e 's|.*/cli/||' -e 's|"$||' | sort -u); do
    [ -e "$root/packages/cli/$dep" ] || missing="$missing packages/cli/$dep"
done
if [ -n "$missing" ]; then
    echo "nano-ros: a runtime crate path-deps into packages/cli, but staging omits:$missing" >&2
    echo "  Carve it back in CARVED above and say why in the header." >&2
    false
fi
refuse -e tools "with it, cmake/bootstrap.cmake would provision submodules inside an install prefix instead of returning early"

files="$(find "$root" -type f | wc -l)"
bytes="$(du -sb "$root" | cut -f1)"
echo "nano-ros: staged the SDK root -> $root ($files files, $bytes bytes)"
