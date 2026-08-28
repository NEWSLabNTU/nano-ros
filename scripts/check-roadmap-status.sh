#!/usr/bin/env bash
# Every ACTIVE roadmap phase must carry a findable status line.
#
# `docs/roadmap/*.md` is the "planned / in-flight work" series; a phase leaves it
# for `archived/` when it completes (CLAUDE.md's three-series convention). While
# it is active, the one thing a reader needs first is where it stands — and the
# one thing a stale phase hides is that it stopped moving.
#
# This gate exists because a one-off pass is not enough. `ecc195ed6` went through
# and gave every active phase a status line; days later four had none again
# (`303-xcdr2-interop`, `336-build-profile-propagation`, `340-build-artifact-reuse`,
# and `292-asi-reference-consumer-revisit` in a shape the others' format missed).
# Nothing asserted the property, so it decayed at the rate new phases are added.
#
# The check is deliberately shallow: it does not judge whether the status is
# CURRENT, only that one exists and is findable near the top. Staleness of the
# CONTENT is a human call — phase-296 says so itself ("IN PROGRESS — but LAST
# RECORDED 2026-07-23, so treat the figures as stale"), which is exactly the
# honesty this gate is meant to make room for, not replace.
#
# Accepted shapes, anywhere in the document:
#     **Status.** …          **Status:** …
#     **Status (DATE).** …   Status: **…**
#     ## Status (DATE) — …   (heading form)
# i.e. a line whose first non-markup word is "Status". All three shapes already
# in the tree pass — the house `**Status.**`, the `Status:` variant, and the
# `## Status` heading. A phase that buries its state in prose does not.
#
# There is no line-window either: `303-xcdr2-interop` states its status at line
# 54, under a long correction preamble that earned its place at the top. A
# window would have flagged a phase that documents itself well.
#
# The accepted set is deliberately the union of what is already written rather
# than one blessed spelling: this gate is here to catch a phase with NO stated
# status, and a first cut that recognised only `**Status`
# flagged `303-xcdr2-interop` — which states its status perfectly well, under a
# heading. Reformatting three phases to satisfy a regex would have been the gate
# bending the tree to itself.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# issue 0726 — `if ! grep -qiE Status` names a specific phase doc as missing its
# status line, so a grep that failed to start files a documentation finding
# against a document that has one. `nros_grep_q` exits 2 and passes -iE through.
# shellcheck source=scripts/lib/grep-q.sh
source scripts/lib/grep-q.sh

missing=0
checked=0

while IFS= read -r doc; do
    checked=$((checked + 1))
    # The rule is stated above as "a line whose first non-markup word is
    # Status", so the match ends at a WORD BOUNDARY. The first spelling of this
    # regex demanded punctuation immediately after `Status` — which rejected
    # `**Status of the work below.**` (phase-341), a line that satisfies the
    # stated rule perfectly well. That is the gate being narrower than the rule
    # it enforces, and the same trap the comment above already describes: a
    # first cut recognising only `**Status` flagged `303-xcdr2-interop`, and
    # reformatting the phase would have been "the gate bending the tree to
    # itself". Widening here keeps every shape listed above passing.
    if ! nros_grep_q -iE '^[[:space:]]*(#+[[:space:]]*)?(\*\*)?Status\b' "$doc"; then
        if [ "$missing" -eq 0 ]; then
            echo "check-roadmap-status: active phase with no findable status line:" >&2
        fi
        echo "  $doc" >&2
        missing=$((missing + 1))
    fi
done < <(git ls-files 'docs/roadmap/phase-*.md')

if [ "$missing" -ne 0 ]; then
    {
        echo
        echo "  An active phase says where it stands, somewhere in the document:"
        echo
        echo "      **Status.** IN PROGRESS — W1/W2 landed <date>; W3 blocked on …"
        echo "      **Status.** Not started."
        echo "      **Status (2026-08-07).** COMPLETE — archive after the follow-ups land."
        echo
        echo "  A finished phase does not need one here at all — move it:"
        echo "      git mv docs/roadmap/<phase>.md docs/roadmap/archived/"
    } >&2
    exit 1
fi

echo "check-roadmap-status: OK ($checked active phase(s) carry a status line)"
