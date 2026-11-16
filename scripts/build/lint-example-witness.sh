#!/usr/bin/env bash
# One witness workspace's lint: `cargo fmt --check`, then clippy, on each of its
# node packages (issue 2595). The sibling of `lint-example-leaf.sh`, which a
# witness cannot use: its node package has no `system.toml` of its own, so
# configuring it as a leaf writes leaf residue (`drop_workspace_members.py`).
#
# Usage (repo root): lint-example-witness.sh <witness-dir>
# Env (exported by `lint-examples.sh`): NIGHTLY, NROS_EXAMPLE_LINT_DIR,
#   NROS_EXAMPLE_PROFILE_FLAGS.
#
# The road is the one that builds it:
#   1. `nros configure` in the workspace renders each image's settings
#      (`build/<group>/<image>_entry/nros-cargo.toml`: patch rows, env knobs,
#      target), and `nros build --dry-run` names the cargo command the default
#      image runs: `--manifest-path <m> --config <settings>` plus any flags
#      (an owned entry's `--features <pkg>_nros_derived/image-<id>`). `<m>` is either a
#      generated entry crate that names the node package as a path dependency,
#      or (an owned entry) the node package itself.
#   2. clippy runs INSIDE the node package, with those settings. Run through a
#      generated entry's manifest instead, clippy compiles the package as a
#      path dependency and lints NOTHING in it (measured: a planted
#      `clippy::eq_op` error went unreported). Only a root package is linted.
#   3. The package's lock is seeded from the registry pins first
#      (`nros dev lock seed`), as `nros build` seeds an owned entry's, so the
#      versions are the image's. Example locks are gitignored.
#
# Warnings print and do not fail, as in `lint-example-leaf.sh`: the lane has
# never denied warnings. A deny-level lint and a format diff fail.
set -euo pipefail
ws="${1:?witness dir}"
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo"
flags="${NROS_EXAMPLE_PROFILE_FLAGS:-}"
unit="$(python3 -c 'import sys; print(sys.argv[1].removeprefix("examples/").replace("/", "-"))' "$ws")"

log="$(mktemp "${TMPDIR:-/tmp}/nros_lint_witness.XXXXXX")"
if ! (cd "$ws" && nros configure) >"$log" 2>&1; then
    echo "linux check: \`nros configure\` failed in $ws" >&2
    cat "$log" >&2
    rm -f "$log"
    exit 1
fi
# The default image's cargo command, from the same dry run `nros build` prints.
if ! (cd "$ws" && nros build --dry-run) >"$log" 2>&1; then
    echo "linux check: \`nros build --dry-run\` failed in $ws" >&2
    cat "$log" >&2
    rm -f "$log"
    exit 1
fi
# The `cargo build` line, from its own word on: an env prefix before it (the
# stack-floor link flags of a bare-metal image) is about linking, not linting.
line="$(grep -oE '(^| )cargo build --manifest-path .*' "$log" | head -1 | sed 's/^ //')"
rm -f "$log"
read -r -a words <<<"${line#cargo build }"
manifest=""; cargo_args=()
i=0
while [ "$i" -lt "${#words[@]}" ]; do
    w="${words[$i]}"
    case "$w" in
        --manifest-path) manifest="${words[$((i + 1))]}"; i=$((i + 2)); continue ;;
        --config)
            # Relative to the workspace; clippy runs from the package.
            c="${words[$((i + 1))]}"
            cargo_args+=(--config "$(cd "$ws" && cd "$(dirname "$c")" && pwd)/$(basename "$c")")
            i=$((i + 2)); continue ;;
        # The image's profile: a board image names one now (it defaults to
        # `release`), and the lane names its own in `$flags`. Cargo refuses
        # `--profile` twice, and every package lints at the lane's profile.
        --profile) i=$((i + 2)); continue ;;
        # profile-literal-ok: flag mapping: recognises the image's profile
        # flag in order to drop it; it names none of its own.
        --profile=* | --release | -r) i=$((i + 1)); continue ;;
    esac
    cargo_args+=("$w")
    i=$((i + 1))
done
if [ -z "$manifest" ] || [ "${#cargo_args[@]}" -eq 0 ]; then
    echo "lint-example-witness: the dry run in $ws named no \`cargo build --manifest-path … --config …\`" >&2
    exit 1
fi

mapfile -t pkgs < <(python3 scripts/lib/example_lint_units.py packages "$ws")
wait $! || { echo "lint-example-witness: listing the packages of $ws failed" >&2; exit 1; }
if [ "${#pkgs[@]}" -eq 0 ]; then
    echo "lint-example-witness: $ws holds no tracked Cargo.toml package" >&2
    exit 1
fi

for pkg_dir in "${pkgs[@]}"; do
    rel="${pkg_dir#"$ws"/}"
    name="$(sed -n 's/^name[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' "$pkg_dir/Cargo.toml" | head -1)"
    # The default image builds this package: it is the manifest itself, or the
    # generated entry names it as a path dependency.
    if [ "$manifest" != "$rel/Cargo.toml" ] \
        && ! grep -qE "path = \"[^\"]*/${rel//\//\\/}\"" "$ws/$manifest"; then
        echo "lint-example-witness: the default image of $ws does not build $rel" >&2
        exit 1
    fi
    nros dev lock seed "$pkg_dir" >/dev/null
    lintdir="$NROS_EXAMPLE_LINT_DIR/$unit-$name"
    (
        cd "$pkg_dir"
        cargo "+$NIGHTLY" fmt --check -p "$name"
        # shellcheck disable=SC2086  # $flags is a word list by design
        cargo clippy --quiet "${cargo_args[@]}" --target-dir "$lintdir" $flags
    )
done
