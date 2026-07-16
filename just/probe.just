set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}

# Clean-system bootstrap probe (issue #204): run the book's documented setup
# steps verbatim in a pristine ubuntu:24.04 container and assert the
# first-node chapter's documented outcome. ~30-60 min cold (builds the nros
# CLI from source inside the container). Knobs: PROBE_BRANCH, PROBE_CLONE_URL,
# PROBE_IMAGE, PROBE_KEEP=1.
[group("main")]
bootstrap:
    ./scripts/probe/run-bootstrap-probe.sh

# Extract the probe script from the book without running it (drift check —
# fails if the book's tagged blocks or substitution anchors changed shape).
[group("main")]
extract out="tmp/probe-steps.sh":
    mkdir -p tmp
    PROBE_EXTRACT_ONLY={{out}} ./scripts/probe/run-bootstrap-probe.sh
