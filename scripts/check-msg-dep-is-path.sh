#!/usr/bin/env bash
#
# RFC-0067 D1 / phase-333 W2 — a generated ROS message crate must be referenced
# as a PATH dependency, never by registry name.
#
# WHY THIS IS STRONGER THAN THE REDIRECT GATE IT REPLACES
#
# In-tree leaves used to declare message deps by REGISTRY NAME:
#
#     std_msgs = { version = "*", default-features = false }
#
# nano-ros publishes nothing to crates.io, and those names are taken by third
# parties (`std_msgs = "0.0.0"`, `builtin_interfaces = "0.0.0"`). The only thing
# standing between a build and a stranger's crate was a `[patch.crates-io]`
# redirect — and cargo loads `.cargo/config.toml` by walking up from the CURRENT
# DIRECTORY, not from the manifest, so `cargo --manifest-path <leaf>` run from
# anywhere else never loaded it. Issue 0378 declared that hole unclosable by
# repo-side config, and it was: no config can be correct for all sixteen leaves
# at once, because each redirects to its own `generated/` tree.
#
# A path dep closes it structurally. Cargo never consults a registry for a path
# dep, from any cwd, whatever a third party publishes. On an unsynced tree the
# target directory is absent and cargo fails CLOSED ("failed to load source"),
# which is the correct outcome — never a silent fall-through to crates.io.
#
# So this gate asserts the property itself rather than the mitigation: no
# registry-named message dep anywhere. It needs no config-chain walk, which also
# makes it immune to the cwd problem that limited its predecessor.
set -uo pipefail

cd "$(dirname "$0")/.."

MSG_CRATES='std_msgs|builtin_interfaces|example_interfaces|geometry_msgs|sensor_msgs|lifecycle_msgs|action_msgs|rosgraph_msgs|nav_msgs|diagnostic_msgs|trajectory_msgs|shape_msgs|stereo_msgs|visualization_msgs|unique_identifier_msgs|test_msgs'

status=0
offenders=0
checked=0

# A registry-style declaration is one carrying a `version` key (bare string or
# table). `{ path = … }` is what we want; `{ version = …, path = … }` still
# registers the name in the crates.io namespace, so it counts as an offender.
while IFS= read -r manifest; do
    checked=$((checked + 1))
    hits="$(grep -nE "^[[:space:]]*($MSG_CRATES)[[:space:]]*=[[:space:]]*(\"|\{[^}]*version)" \
        "$manifest" 2>/dev/null || true)"
    [ -z "$hits" ] && continue
    while IFS= read -r hit; do
        [ -z "$hit" ] && continue
        offenders=$((offenders + 1))
        line="${hit%%:*}"
        crate="$(printf '%s' "$hit" | grep -oE "($MSG_CRATES)" | head -1)"
        echo "FAIL: $manifest:$line — \`$crate\` is a registry dep." >&2
        echo "      Declare it as a path dep:  $crate = { path = \"generated/$crate\", default-features = false }" >&2
        echo "      (RFC-0067 D1: a registry name resolves against PUBLIC crates.io whenever the" >&2
        echo "       [patch.crates-io] redirect is not in the loaded config chain — which depends on cwd.)" >&2
        status=1
    done <<<"$hits"
done < <(git ls-files '*/Cargo.toml' 'Cargo.toml')

# The patch entries are retired with the registry names: a path dep needs none,
# and a leftover entry makes cargo warn about an unused patch.
patch_leftovers="$(git grep -nE "^($MSG_CRATES) = .*# nros-managed" -- '*config.toml' || true)"
if [ -n "$patch_leftovers" ]; then
    echo "FAIL: retired message-crate [patch.crates-io] entries remain (RFC-0067 D1):" >&2
    printf '%s\n' "$patch_leftovers" | sed 's/^/      /' >&2
    status=1
fi

if [ "$status" -ne 0 ]; then
    echo "check-msg-dep-is-path: $offenders registry-named message dep(s) across $checked manifest(s)" >&2
    exit 1
fi
echo "msg deps are path deps: $checked manifest(s) clean"
