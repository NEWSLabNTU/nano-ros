#!/usr/bin/env bash

# Shared helpers for cargo-nextest run options and optional profiling.

NROS_NEXTEST_PROFILE_ARGS=()

nros_nextest_run_profile_name() {
    printf '%s\n' "${NROS_NEXTEST_RUN_PROFILE:-default}"
}

nros_nextest_run_profile_args() {
    local profile
    profile="$(nros_nextest_run_profile_name)"
    if [ "$profile" != "default" ]; then
        printf '%s\n' "-P" "$profile"
    fi
}

nros_nextest_fail_fast_args() {
    local profile
    profile="$(nros_nextest_run_profile_name)"
    if [ "$profile" = "default" ]; then
        printf '%s\n' "--no-fail-fast"
    fi
}

nros_nextest_junit_path() {
    printf 'target/nextest/%s/junit.xml\n' "$(nros_nextest_run_profile_name)"
}

nros_nextest_profile_enabled() {
    [ -n "${NROS_NEXTEST_PROFILE:-}" ] && [ "${NROS_NEXTEST_PROFILE:-}" != "0" ]
}

nros_nextest_profile_begin() {
    local scope="$1"

    if ! nros_nextest_profile_enabled; then
        return 0
    fi

    local timestamp run_dir latest_link group_by config_file
    timestamp="$(date +%Y%m%d-%H%M%S)"
    if [ -n "${NROS_NEXTEST_PROFILE_DIR:-}" ]; then
        run_dir="$NROS_NEXTEST_PROFILE_DIR"
        latest_link="${NROS_NEXTEST_PROFILE_DIR}-latest"
    else
        run_dir="tmp/nextest-profile-${scope}-${timestamp}"
        latest_link="tmp/nextest-profile-${scope}-latest"
    fi

    mkdir -p "$run_dir"
    ln -sfn "$(basename "$run_dir")" "$latest_link"

    export NROS_NEXTEST_PROFILE_RUN_DIR="$run_dir"
    export NEXTEST_EXPERIMENTAL_RECORD=1
    export NEXTEST_STATE_DIR="$run_dir/state"
    export NROS_NEXTEST_JUNIT_PATH="$(nros_nextest_junit_path)"
    config_file="${NROS_NEXTEST_PROFILE_CONFIG:-.config/nextest-profile.toml}"
    export NROS_NEXTEST_PROFILE_CONFIG="$config_file"
    NROS_NEXTEST_PROFILE_ARGS=(--user-config-file "$config_file")
    group_by="${NROS_NEXTEST_TRACE_GROUP_BY:-slot}"
    case "$group_by" in
        slot|binary)
            ;;
        *)
            echo "warning: invalid NROS_NEXTEST_TRACE_GROUP_BY='$group_by'; using slot" >&2
            group_by="slot"
            ;;
    esac
    export NROS_NEXTEST_TRACE_GROUP_BY_EFFECTIVE="$group_by"

    {
        printf 'NROS_NEXTEST_PROFILE=%s\n' "${NROS_NEXTEST_PROFILE:-}"
        printf 'NROS_NEXTEST_PROFILE_DIR=%s\n' "${NROS_NEXTEST_PROFILE_DIR:-}"
        printf 'NROS_NEXTEST_REPLAY_LOG=%s\n' "${NROS_NEXTEST_REPLAY_LOG:-}"
        printf 'NROS_NEXTEST_TRACE_GROUP_BY=%s\n' "${NROS_NEXTEST_TRACE_GROUP_BY:-}"
        printf 'NROS_NEXTEST_PROFILE_KEEP_STATE=%s\n' "${NROS_NEXTEST_PROFILE_KEEP_STATE:-}"
        printf 'NROS_NEXTEST_RUN_PROFILE=%s\n' "$(nros_nextest_run_profile_name)"
        printf 'NROS_NEXTEST_JUNIT_PATH=%s\n' "$NROS_NEXTEST_JUNIT_PATH"
        printf 'NEXTEST_STATE_DIR=%s\n' "$NEXTEST_STATE_DIR"
        printf 'NROS_NEXTEST_PROFILE_CONFIG=%s\n' "$NROS_NEXTEST_PROFILE_CONFIG"
    } > "$run_dir/env.txt"

    echo "Nextest profiling enabled: $run_dir"
}

nros_nextest_profile_write_command() {
    if ! nros_nextest_profile_enabled; then
        return 0
    fi

    local run_dir
    run_dir="${NROS_NEXTEST_PROFILE_RUN_DIR:-}"
    if [ -z "$run_dir" ] || [ ! -d "$run_dir" ]; then
        return 0
    fi

    printf '%q ' "$@" > "$run_dir/command.txt"
    printf '\n' >> "$run_dir/command.txt"
}

nros_nextest_profile_finish() {
    if ! nros_nextest_profile_enabled; then
        return 0
    fi

    local run_dir group_by
    run_dir="${NROS_NEXTEST_PROFILE_RUN_DIR:-}"
    if [ -z "$run_dir" ] || [ ! -d "$run_dir" ]; then
        echo "warning: nextest profile directory missing; skipping artifact export" >&2
        return 0
    fi

    local junit
    junit="${NROS_NEXTEST_JUNIT_PATH:-$(nros_nextest_junit_path)}"
    if [ -f "$junit" ]; then
        cp "$junit" "$run_dir/junit.xml"
    else
        echo "warning: $junit missing; skipping JUnit copy" >&2
    fi

    if ! cargo nextest store export latest "${NROS_NEXTEST_PROFILE_ARGS[@]}" \
        --archive-file "$run_dir/nextest-run.zip"; then
        echo "warning: failed to export nextest recording archive" >&2
    fi

    group_by="${NROS_NEXTEST_TRACE_GROUP_BY_EFFECTIVE:-slot}"
    if ! cargo nextest store export-chrome-trace latest "${NROS_NEXTEST_PROFILE_ARGS[@]}" \
        --group-by "$group_by" -o "$run_dir/nextest-trace.json"; then
        echo "warning: failed to export nextest chrome trace" >&2
    fi

    if [ -n "${NROS_NEXTEST_REPLAY_LOG:-}" ] && [ "${NROS_NEXTEST_REPLAY_LOG:-}" != "0" ]; then
        if ! cargo nextest replay "${NROS_NEXTEST_PROFILE_ARGS[@]}" \
            --success-output immediate \
            --failure-output immediate \
            --status-level all \
            --final-status-level all \
            --no-pager > "$run_dir/nextest-replay.log"; then
            echo "warning: failed to write nextest replay log" >&2
        fi
    fi

    if [ -z "${NROS_NEXTEST_PROFILE_KEEP_STATE:-}" ] || \
        [ "${NROS_NEXTEST_PROFILE_KEEP_STATE:-}" = "0" ]; then
        rm -rf "$run_dir/state"
    fi

    echo "Nextest profile artifacts: $run_dir"
}
