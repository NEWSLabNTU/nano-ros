#!/usr/bin/env bash
# scripts/probe/verify-starter-image.sh <image-ref> [--pull]
#
# The `image` probe track -- RFC-0127 D7, phase-551 W5/W6. Run against a
# released starter tag before `latest` is moved to it (host-tests.yml image-track, through
# scripts/ci/starter-latest.sh), or against a locally built one
# (`just probe image nano-ros-starter:local`).
#
#   size     the image's COMPRESSED size against its budget (2 GB `starter`,
#            5 GB `<ver>-zephyr`). With --pull it is what the registry
#            serves (the sum of the manifest's layer sizes); for a local image
#            it is `docker save | gzip -6`, the estimate starter-image.sh uses.
#            Over budget is red.
#   pull     with --pull, the local copy is removed and the pull timed. The
#            time is the probe runner's link and the registry's, not a lab's,
#            so it is printed, never gated: on a fixed link the time is a
#            function of the size, and the size is the gate. What a ROOM
#            would take is printed beside it, for the link RFC-0127 Q9 ruled
#            (100 Mbit/s shared by 30 seats).
#   the page every `probe=` block of book/src/start/starter-image.md, in
#            order. Blocks 20-29 are the SEAT's (typed inside the container):
#            they run in the image with `--network none`, and every line of
#            their `probe-expect` block must appear. Every other block is the
#            HOST's (pull, run, mount, digest, save/load) and runs here, with
#            the page's image name replaced by <image-ref> and `-it` dropped
#            to `-i` with no input, so each container starts and exits.
#            Without --pull a `docker pull` would fetch a different image (or
#            none, for a local tag), so there it becomes `docker image
#            inspect`, and block 40, which reads a registry digest a local
#            build has not got, does not run.
#   course   the page's devcontainer.json, with the digest put in, parses
#            and names that digest, and `nros doctor` in the image names the
#            image on its `container` line (RFC-0127 D4).
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=scripts/lib/grep-q.sh
. "$REPO_ROOT/scripts/lib/grep-q.sh"
NAME=verify-starter-image
ENGINE="${NROS_IMAGE_ENGINE:-docker}"
PAGE="$REPO_ROOT/book/src/start/starter-image.md"
# RFC-0127 Q9 (ruled 2026-11-12): the lab link.
ROOM_SEATS=30
ROOM_BYTES_PER_S=12500000 # 100 Mbit/s

REF=""
PULL=0
while [ $# -gt 0 ]; do
    case "$1" in
        --pull) PULL=1; shift ;;
        -*) echo "usage: verify-starter-image.sh <image-ref> [--pull]" >&2; exit 2 ;;
        *) REF="$1"; shift ;;
    esac
done
[ -n "$REF" ] || { echo "usage: verify-starter-image.sh <image-ref> [--pull]" >&2; exit 2; }
case "$REF" in
    *-zephyr) budget=$((5 * 1000 * 1000 * 1000)) ;;
    *) budget=$((2 * 1000 * 1000 * 1000)) ;;
esac
gb() { awk -v b="$1" 'BEGIN { printf "%.2f GB", b / 1e9 }'; }
mins() { awk -v s="$1" 'BEGIN { printf "%.0f min", s / 60 }'; }

fail=0
red() { echo "$NAME: FAIL $*" >&2; fail=1; }

# --- pull and size ------------------------------------------------------------
if [ "$PULL" = 1 ]; then
    "$ENGINE" image rm -f "$REF" >/dev/null 2>&1 || true
    t0=$(date +%s)
    "$ENGINE" pull --quiet "$REF" >/dev/null
    t1=$(date +%s)
    echo "$NAME: pulled $REF in $((t1 - t0)) s on this runner's link"
    manifest="$("$ENGINE" manifest inspect -v "$REF")"
    compressed="$(python3 -c 'import json, sys
d = json.loads(sys.stdin.read())
d = d[0] if isinstance(d, list) else d
m = d.get("SchemaV2Manifest") or d.get("OCIManifest") or d
print(sum(layer["size"] for layer in m["layers"]))' <<<"$manifest")"
    how="the registry's layers"
else
    "$ENGINE" image inspect "$REF" >/dev/null 2>&1 || {
        echo "$NAME: no image '$REF' here; pass --pull for a published one" >&2
        exit 1
    }
    compressed="$("$ENGINE" save "$REF" | gzip -6 | wc -c)"
    how="gzip -6 of docker save"
fi
room=$((compressed * ROOM_SEATS / ROOM_BYTES_PER_S))
echo "$NAME: $REF is $(gb "$compressed") compressed ($how); budget $(gb "$budget")"
echo "$NAME: a room of $ROOM_SEATS seats on 100 Mbit/s pulls it in about $(mins "$room")"
[ "$compressed" -le "$budget" ] || red "size: over the RFC-0127 D7 budget"

# --- the page's blocks ------------------------------------------------------------
steps_dir="$(mktemp -d)"
chmod 755 "$steps_dir" # the image runs as `nros`, another uid
trap 'rm -rf "$steps_dir"' EXIT
python3 - "$REPO_ROOT" "$PAGE" "$steps_dir" "$REF" "$PULL" <<'PYEOF'
import importlib.util, sys
from pathlib import Path
root, page, out, ref, pull = (Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3]),
                              sys.argv[4], sys.argv[5] == "1")
spec = importlib.util.spec_from_file_location(
    "extract_book_steps", root / "scripts/probe/extract-book-steps.py")
ebs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ebs)
steps, expects = ebs.extract_blocks(page)
IMAGE = "ghcr.io/newslabntu/nano-ros-starter"
order = []
for st in sorted(steps, key=lambda s: s["order"]):
    n, body = st["order"], st["body"]
    if 20 <= n < 30:
        kind = "seat"
    else:
        kind = "host"
        names = body.count(IMAGE + ":latest") + body.count(IMAGE + ":<version>")
        if names == 0 and "nano-ros-starter.tar.gz" not in body:
            sys.exit(f"{page}: host step probe={n} names no image")
        body = (body.replace(IMAGE + ":latest", ref)
                    .replace(IMAGE + ":<version>", ref)
                    .replace("docker run --rm -it", "docker run --rm -i"))
        if not pull:
            body = body.replace("docker pull ", "docker image inspect --format '{{.Id}}' ")
    # A reader's shell: -e, but no -u (ROS's setup.bash reads unset variables).
    (out / f"{kind}-{n}.sh").write_text("set -eo pipefail\n" + body + "\n")
    order.append(f"{kind} {n}")
(out / "order").write_text("\n".join(order) + "\n")
(out / "expect.txt").write_text("\n".join(l for e in expects for l in e["lines"]) + "\n")
# The course recipe: the page's one devcontainer.json block.
blocks, cur = [], None
for line in page.read_text().split("\n"):
    if cur is None and line.strip() == "```json":
        cur = []
    elif cur is not None and line.startswith("```"):
        blocks.append("\n".join(cur)); cur = None
    elif cur is not None:
        cur.append(line)
if len(blocks) != 1:
    sys.exit(f"{page}: expected one ```json block (the devcontainer.json), found {len(blocks)}")
(out / "devcontainer.json").write_text(blocks[0] + "\n")
PYEOF

work="$steps_dir/work"
mkdir -p "$work"
chmod 777 "$work" # block 30 mounts it as /work for `nros`
while read -r kind n; do
    case "$kind" in
        seat)
            if "$ENGINE" run --rm --network none -v "$steps_dir:/probe:ro" "$REF" \
                    bash "/probe/seat-$n.sh" >"$steps_dir/seat-$n.log" 2>&1; then
                seat_log="$(cat "$steps_dir/seat-$n.log")"
                while IFS= read -r want; do
                    [ -n "$want" ] || continue
                    nros_grep_q -F -- "$want" <<<"$seat_log" \
                        || red "seat: probe=$n never printed '$want'"
                done <"$steps_dir/expect.txt"
                echo "$NAME: OK   probe=$n (seat, no network)"
            else
                tail -25 "$steps_dir/seat-$n.log" >&2
                red "seat: probe=$n failed with no network"
            fi
            ;;
        host)
            if [ "$n" = 40 ] && [ "$PULL" = 0 ]; then
                echo "$NAME: --   probe=$n reads a registry digest; a local build has none (run with --pull)"
                continue
            fi
            if (cd "$work" && bash "$steps_dir/host-$n.sh" </dev/null >"$steps_dir/host-$n.log" 2>&1); then
                echo "$NAME: OK   probe=$n (host)"
            else
                tail -15 "$steps_dir/host-$n.log" >&2
                red "host: probe=$n failed"
            fi
            ;;
    esac
done <"$steps_dir/order"

# --- course: the devcontainer.json names the digest; doctor names the image ---------
if [ "$PULL" = 1 ]; then
    digest="$("$ENGINE" image inspect --format '{{index .RepoDigests 0}}' "$REF")"
    sed "s|sha256:<digest>|${digest#*@}|" "$steps_dir/devcontainer.json" >"$steps_dir/dc.json"
    if python3 -c 'import json, sys
d = json.load(open(sys.argv[1]))
assert d["image"] == sys.argv[2], (d["image"], sys.argv[2])
assert d["workspaceFolder"] == "/work" and d["remoteUser"] == "nros", d' "$steps_dir/dc.json" "$digest"; then
        echo "$NAME: OK   course: devcontainer.json names $digest"
    else
        red "course: the page's devcontainer.json does not name the image by digest"
    fi
fi
doctor="$("$ENGINE" run --rm --network none "$REF" nros doctor 2>&1 || true)"
if nros_grep_q -F -- "nros doctor: container $REF " <<<"$doctor"; then
    echo "$NAME: OK   course: nros doctor names $REF"
else
    red "course: nros doctor's container line does not name $REF"
fi

[ "$fail" = 0 ] && echo "$NAME: OK   $REF"
exit "$fail"
