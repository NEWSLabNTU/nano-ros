#!/usr/bin/env bash
# Issue 1671 -- `nros_fixture_row_failed` for a caller that is not a bash
# function: a generated make recipe (zephyr-fixture-make-driver.sh).
#
#   fixture-row-failed.sh <label> <rc>
#
# Exits 0 after recording the row when the lane keeps going
# (NROS_FIXTURE_FAILED_ROWS set), else exits <rc> -- so a recipe that ends in
# `bash fixture-row-failed.sh <label> "$status"; exit $?` behaves exactly as
# `exit "$status"` did whenever the lane did not ask to keep going.
set -uo pipefail
if [ "$#" -ne 2 ]; then
    echo "usage: $0 <label> <rc>" >&2
    exit 2
fi
# shellcheck source=scripts/build/fixture-row-ledger.sh
source "$(dirname "${BASH_SOURCE[0]}")/fixture-row-ledger.sh"
nros_fixture_row_failed "$1" "$2"
