#!/usr/bin/env bash
# Install the PINNED `just` from its GitHub release asset — the ONE spelling of
# "get `just` onto a CI machine" (issue 1760).
#
#   bash scripts/ci/install-just.sh --to <dir>      install into <dir>
#   bash scripts/ci/install-just.sh --print-version print the pin and exit
#
# Used by `.github/workflows/probe.yml` (both tracks, bare hosted runners) and
# by both CI images (`ci/docker/ci-base/Dockerfile`,
# `ci/docker/zephyr-ros/Dockerfile`). `check-workflow-just-provisioning` refuses
# the spelling this replaced anywhere under `.github/` or `ci/docker/`.
#
# What it replaced, and why each property is here:
#
#   * `curl -sSf https://just.systems/install.sh | bash` fetched an INSTALLER
#     SCRIPT from a third-party host on every run. One 403 on that script (probe
#     run 37904203449) stopped a lane before it produced a verdict, and curl
#     does not retry a 403 without `--retry-all-errors`. Now the only fetch is
#     the release asset itself, from github.com, retried on every error class.
#   * The installer chose the version at run time unless told otherwise, and
#     the probe sites did not tell it. The version is pinned here, once.
#   * Nothing verified what was downloaded. The asset's SHA-256 is pinned beside
#     the version and checked before anything is extracted.
#
# Bump: change JUST_VERSION and both digests together — read them off the
# release (`gh release view <v> -R casey/just --json assets`), never off a
# local download alone. A ci-base / zephyr image rebuild follows on merge,
# because this file is in `images.yml`'s path filter and content tag.
set -euo pipefail

JUST_VERSION=1.57.0
sha256_for() {
    case "$1" in
        x86_64-unknown-linux-musl)
            echo 45b548094283cb9739af8f13273b8cddeee869f5b4ef2bb631b1f311cb566155 ;;
        aarch64-unknown-linux-musl)
            echo f225044a81adea6e0b3a8b9370aaf374e6af76c8735ae263ac993df55fd137ec ;;
        *) return 1 ;;
    esac
}

to=""
while [ $# -gt 0 ]; do
    case "$1" in
        --to) to="${2:?--to needs a directory}"; shift 2 ;;
        --print-version) echo "$JUST_VERSION"; exit 0 ;;
        *) echo "install-just: unknown argument: $1" >&2; exit 2 ;;
    esac
done
[ -n "$to" ] || { echo "install-just: --to <dir> is required" >&2; exit 2; }

case "$(uname -m)" in
    x86_64|amd64) triple=x86_64-unknown-linux-musl ;;
    aarch64|arm64) triple=aarch64-unknown-linux-musl ;;
    *) echo "install-just: no pinned asset for $(uname -m)" >&2; exit 1 ;;
esac
want="$(sha256_for "$triple")"

asset="just-${JUST_VERSION}-${triple}.tar.gz"
url="https://github.com/casey/just/releases/download/${JUST_VERSION}/${asset}"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# `--retry-all-errors` is the point: without it curl retries only what it
# classifies as transient, and a 403/5xx from a CDN edge is not on that list.
curl --proto '=https' --tlsv1.2 -fsSL \
    --retry 5 --retry-delay 5 --retry-all-errors --connect-timeout 30 \
    -o "$tmp/$asset" "$url"

got="$(sha256sum "$tmp/$asset" | cut -d' ' -f1)"
if [ "$got" != "$want" ]; then
    echo "install-just: SHA-256 mismatch for $asset" >&2
    echo "  want $want" >&2
    echo "  got  $got" >&2
    exit 1
fi

tar -xzf "$tmp/$asset" -C "$tmp" just
mkdir -p "$to"
install -m0755 "$tmp/just" "$to/just"
reported="$("$to/just" --version)"
if [ "$reported" != "just $JUST_VERSION" ]; then
    echo "install-just: $to/just reports '$reported', want 'just $JUST_VERSION'" >&2
    exit 1
fi
echo "install-just: just $JUST_VERSION -> $to/just"
