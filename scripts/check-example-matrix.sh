#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

rmw_names='^(zenoh|xrce|dds|cyclonedds|uorb)$'

allowed_roots=(
  # Phase 118.G is still in flight and owns the remaining bare-metal
  # Rust legacy roots.
  "examples/esp32/rust/zenoh"
  "examples/qemu-arm-baremetal/rust/dds"
  "examples/qemu-arm-baremetal/rust/zenoh"
  "examples/qemu-esp32-baremetal/rust/dds"
  "examples/qemu-esp32-baremetal/rust/zenoh"
  "examples/stm32f4/rust/zenoh"

  # Phase 118.H carve-outs. px4's sub-dir axis is the transport integration
  # CASE (uORB vs XRCE — PX4's two native messaging surfaces), not the retired
  # per-RMW layout, so these legitimately keep a `<name>` matching an RMW token.
  # `examples/px4/rust/xrce` (the PX4 SITL XRCE e2e, commit 1031f07e4) was missed
  # when it landed — see docs/issues/0051. Add a cpp/xrce line if/when that case
  # lands.
  "examples/px4/cpp/uorb"
  "examples/px4/rust/uorb"
  "examples/px4/rust/xrce"

  # One-board Zephyr CycloneDDS reference, documented in CLAUDE.md.
  # Both languages carve out — the rust sibling was missed when the cpp one
  # landed (same single-board reference shape).
  "examples/zephyr/cpp/cyclonedds"
  "examples/zephyr/rust/cyclonedds"
)

is_allowed() {
  local path="$1"
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

echo "Example matrix lint passed."
