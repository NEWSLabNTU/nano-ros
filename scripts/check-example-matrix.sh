#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

rmw_names='^(zenoh|xrce|dds|cyclonedds|uorb)$'

allowed_roots=(
  # (phase-277 W7 pruned the lines whose dirs no longer exist:
  # qemu-arm-baremetal/rust/zenoh, qemu-esp32-baremetal/rust/zenoh,
  # stm32f4/rust/zenoh. Issue 0314 then removed the last two bare-metal
  # `rust/dds` roots along with every other abandoned per-RMW tree — they held
  # only untracked `generated/` output — so Phase 118.G's carve-outs are gone
  # too. Nothing outside px4 and the zephyr cyclonedds pair is exempt now.)

  # px4 (Phase 118.H) is exempted STRUCTURALLY in is_allowed(), not per-case —
  # see docs/issues/archived/0295. px4 is the one platform whose `examples/px4/<lang>/<name>`
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
  # px4 transport-axis exemption (issue #295): `examples/px4/<lang>/<transport>`
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
  # phase-300 W3.2 — tracked payload via the git index (the -not -path
  # form still DESCENDED build trees before -quit could fire).
  git ls-files -- "$path" | grep -qv '^$'
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
  # Derived from tracked paths rather than walked: `find` over examples/ stats
  # every build tree on the way down (7m36s vs 0.8s, measured).
  git ls-files examples |
    awk -F/ -v re="$rmw_names" 'NF>=5 && $4 ~ re { print $1"/"$2"/"$3"/"$4 }' |
    sort -u
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
done < <(git ls-files examples | awk -F/ 'NF>=3 { print $1"/"$2 }' | sort -u)

# Tier 3: every workspace (base <lang> + ws-*), bridge and template.
while IFS= read -r dir; do
  require_readme "$dir"
done < <(git ls-files examples/workspaces examples/bridges examples/templates \
           | awk -F/ 'NF>=4 { print $1"/"$2"/"$3 }' | sort -u)

if (( ${#readme_failures[@]} > 0 )); then
  echo "Missing README.md (RFC-0026 README tiers):" >&2
  printf '  %s\n' "${readme_failures[@]}" >&2
  echo >&2
  echo "Platform roots, workspaces, bridges and templates each need a README.md;" >&2
  echo "see docs/design/0026-example-directory-layout.md." >&2
  exit 1
fi

echo "Example matrix lint passed."
