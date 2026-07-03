#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

rmw_names='^(zenoh|xrce|dds|cyclonedds|uorb)$'

allowed_roots=(
  # Phase 118.G is still in flight and owns the remaining bare-metal
  # Rust legacy roots. (phase-277 W7 pruned the lines whose dirs no
  # longer exist: qemu-arm-baremetal/rust/zenoh,
  # qemu-esp32-baremetal/rust/zenoh, stm32f4/rust/zenoh.)
  "examples/qemu-arm-baremetal/rust/dds"
  "examples/qemu-esp32-baremetal/rust/dds"

  # px4 (Phase 118.H) is exempted STRUCTURALLY in is_allowed(), not per-case —
  # see docs/issues/archived/0051. px4 is the one platform whose `examples/px4/<lang>/<name>`
  # sub-dir axis is a transport integration CASE (uORB vs XRCE — PX4's two native
  # messaging surfaces), not the retired per-RMW layout. New px4 transport cases
  # therefore need NO carve-out line here.

  # One-board Zephyr CycloneDDS reference, documented in CLAUDE.md.
  # Both languages carve out — the rust sibling was missed when the cpp one
  # landed (same single-board reference shape).
  "examples/zephyr/cpp/cyclonedds"
  "examples/zephyr/rust/cyclonedds"
)

is_allowed() {
  local path="$1"
  # px4 transport-axis exemption (issue #51): `examples/px4/<lang>/<transport>`
  # (uORB / XRCE) is px4's legitimate integration-case axis, not the retired
  # per-RMW layout — exempt the whole platform so new transport cases need no
  # per-case carve-out line.
  if [[ "$path" == examples/px4/* ]]; then
    return 0
  fi
  local allowed
  for allowed in "${allowed_roots[@]}"; do
    if [[ "$path" == "$allowed" ]]; then
      return 0
    fi
  done
  return 1
}

has_example_payload() {
  local path="$1"
  find "$path" -mindepth 1 -type f \
    ! -path '*/build/*' \
    ! -path '*/build-*/*' \
    ! -path '*/target/*' \
    ! -path '*/target-*/*' \
    ! -path '*/generated/*' \
    ! -name '.nros-*' \
    -print -quit | grep -q .
}

failures=()
while IFS= read -r dir; do
  if is_allowed "$dir"; then
    continue
  fi
  if ! has_example_payload "$dir"; then
    continue
  fi
  failures+=("$dir")
done < <(
  find examples -mindepth 3 -maxdepth 3 -type d |
    awk -F/ -v re="$rmw_names" '$4 ~ re { print }' |
    sort
)

if (( ${#failures[@]} > 0 )); then
  echo "Retired examples/<platform>/<language>/<rmw>/ roots found:" >&2
  printf '  %s\n' "${failures[@]}" >&2
  echo >&2
  echo "Move cases to examples/<platform>/<language>/<case>/ and select RMW at build time," >&2
  echo "or document an explicit carve-out in scripts/check-example-matrix.sh." >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# README tier lint (phase-277 W7, RFC-0026 "README tiers"): every platform
# root, every workspaces/ws-* + base workspace, every bridges/* and every
# templates/* must carry a README.md. Canonical per-role examples
# (<plat>/<lang>/<case>) deliberately do NOT need one — the platform README
# covers them.
# ---------------------------------------------------------------------------
readme_failures=()

require_readme() {
  local dir="$1"
  if [ ! -f "$dir/README.md" ]; then
    readme_failures+=("$dir")
  fi
}

# Tier 2: per-platform roots + the sibling-category roots (every first-level
# dir under examples/).
while IFS= read -r dir; do
  require_readme "$dir"
done < <(find examples -mindepth 1 -maxdepth 1 -type d | sort)

# Tier 3: every workspace (base <lang> + ws-*), bridge and template.
while IFS= read -r dir; do
  require_readme "$dir"
done < <(find examples/workspaces examples/bridges examples/templates \
           -mindepth 1 -maxdepth 1 -type d | sort)

if (( ${#readme_failures[@]} > 0 )); then
  echo "Missing README.md (RFC-0026 README tiers):" >&2
  printf '  %s\n' "${readme_failures[@]}" >&2
  echo >&2
  echo "Platform roots, workspaces, bridges and templates each need a README.md;" >&2
  echo "see docs/design/0026-example-directory-layout.md." >&2
  exit 1
fi

echo "Example matrix lint passed."
