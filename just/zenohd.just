set working-directory := '..'

# zenohd module — build the zenohd router from the pinned submodule.

default:
    @just --list --justfile {{source_file()}}

# Provision zenohd (skipped if already present).
#
# Phase 187.6: a thin caller of `nros setup` — fetches the prebuilt zenohd from
# the nano-ros-sdk Releases into build/zenohd/bin/ (or source-builds from the
# index recipe on an unsupported host), then symlinks the flat
# build/zenohd/zenohd path the harness + other recipes read. Version lives only
# in nros-sdk-index.toml. Requires the nros CLI (`just setup base` builds it);
# `just zenohd build` still source-builds from the submodule.
[group("setup")]
setup:
    #!/usr/bin/env bash
    set -euo pipefail
    # phase-334 W2.b step 2 — the zenohd cache path comes from the ONE
    # derivation (RFC-0070 R3), not a literal in each recipe. Step 2 keeps
    # today's path; step 3 is what moves it.
    source scripts/build/build-root.sh
    zdir="$(nros_build_dir zenohd)"
    # #189 — the presence check is VERSION-AWARE via the install's
    # .nros-provenance: a bare `-x` skip pinned the pre-serial 1.7.2-nros1
    # binary forever after the index moved on (the `--version` output is
    # identical across nros revisions, so provenance is the only tell).
    want="$(awk '/^\[tool.zenohd\]/{f=1;next} /^\[/{f=0} f && /^version =/{gsub(/"/,"",$3); print $3; exit}' nros-sdk-index.toml)"
    have=""
    if [ -f $zdir/.nros-provenance ]; then
        have="$(sed -n 's/^version = "\(.*\)"/\1/p' $zdir/.nros-provenance | head -1)"
    fi
    if [ -x $zdir/zenohd ] && [ -n "$want" ] && [ "$have" = "$want" ]; then
        echo "zenohd present: $zdir/zenohd ($have)"
        exit 0
    fi
    if [ -x $zdir/zenohd ]; then
        echo "zenohd present but $have != index $want — reprovisioning."
        rm -rf $zdir
    fi
    # issue 0388 — REUSE the `nros setup … --rmw zenoh` store copy before
    # provisioning a second one. `--prefix` installs are workspace-local and NOT
    # lock-tracked, so `nros setup --tool` plans against $zdir alone: on a
    # host where zenohd has no prebuilt (every host today, issue 0374) that means
    # SOURCE-BUILDING zenoh a second time, minutes and hundreds of MB, for a
    # binary already sitting in the store at the same pinned version. Symlink to
    # it instead. $zdir stays the override slot: a locally built or
    # patched zenohd there still wins, here and in the harness's resolver.
    store_root="${NROS_HOME:-$HOME/.nros}/sdk/zenohd/$want/bin/zenohd"
    if [ -x "$store_root" ]; then
        echo "zenohd $want present in the nros store — linking, not rebuilding."
        mkdir -p $zdir
        ln -sfn "$store_root" $zdir/zenohd
        printf 'version = "%s"\nsource = "nros-store"\n' "$want" >$zdir/.nros-provenance
        $zdir/zenohd --version 2>&1 | sed 's/\x1b\[[0-9;]*m//g' | grep -E '^zenohd v' | tail -1 || true
        exit 0
    fi
    source scripts/build/cargo.sh
    nros="$(nros_cli_bin)"
    "$nros" setup --tool zenohd --prefix "$(pwd)/build/zenohd" --index "$(pwd)/nros-sdk-index.toml"
    # Bridge the store bin/ layout to the flat path consumers expect.
    ln -sf bin/zenohd $zdir/zenohd
    $zdir/zenohd --version 2>&1 | sed 's/\x1b\[[0-9;]*m//g' | grep -E '^zenohd v' | tail -1 || true

# Diagnose zenohd install status (read-only).
[group("setup")]
doctor:
    #!/usr/bin/env bash
    set +e
    source scripts/build/build-root.sh
    zdir="$(nros_build_dir zenohd)"
    if [ -x $zdir/zenohd ]; then
        # zenohd logs startup to stderr before printing the version line;
        # strip ANSI escapes and grab the last line that looks like a version.
        ver=$($zdir/zenohd --version 2>&1 \
              | sed 's/\x1b\[[0-9;]*m//g' \
              | grep -E '^zenohd v' \
              | tail -1 || true)
        echo "  [OK] zenohd: $zdir/zenohd${ver:+ ($ver)}"
        exit 0
    else
        echo "  [MISSING] zenohd (run: just zenohd setup)"
        exit 1
    fi

# Build zenohd from the pinned submodule (alias for `just zenohd setup`).
[group("main")]
build:
    ./scripts/zenohd/build.sh

# Run the built zenoh router. Blocks the shell until Ctrl-C.
# Accepts any zenohd flags as positional args (forwarded verbatim).
[group("main")]
run *args:
    #!/usr/bin/env bash
    set -e
    source scripts/build/build-root.sh
    zdir="$(nros_build_dir zenohd)"
    if [ ! -x $zdir/zenohd ]; then
        echo "ERROR: $zdir/zenohd not found. Run: just zenohd setup" >&2
        exit 1
    fi
    exec ./build/zenohd/zenohd {{args}}

# Clean zenohd build artifacts.
[group("main")]
clean:
    ./scripts/zenohd/build.sh --clean
