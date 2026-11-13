# shellcheck shell=bash
#
# Mint a self-hosted runner REGISTRATION token, and refuse anything that is not
# one.
#
# Every runner script minted its own with `gh api … --jq .token`, and none asked
# what came back. Two ways that goes wrong, both quietly:
#
#   - the call FAILED but printed: `gh api` writes the response body to stdout
#     on an HTTP error as well, so `TOKEN="$(gh api … || true)"` keeps an error
#     body when the API is having a bad minute. The loop's `[ -n "$TOKEN" ]` said
#     yes to it;
#   - the value is not one line. The runner's `config.sh` puts the token in an
#     HTTP header, and .NET refuses the whole registration with "New-line
#     characters are not allowed in header values". On 2026-11-13 host 118's
#     loop met that three times in a row, after 24 good jobs, and stopped; a
#     mint a few minutes later was a clean 29-character token.
#
# So: exit status AND shape, retried with a back-off, because a bad minute at
# the API is the commonest cause and costs nothing to wait out. The token is
# never printed in a diagnostic, only its length and line count.
#
#   nros_mint_runner_token <script-name> <owner/repo> [attempts]
#
# Prints the token on success. Returns 1, with the reason on stderr, when every
# attempt failed. Never exits: the caller decides whether that is fatal.

# The shape of a registration token. GitHub documents none; every one seen is
# 29 upper-case alphanumerics. Wide enough not to break on a longer one, narrow
# enough that an error body, `null`, or a value with a newline cannot pass.
NROS_RUNNER_TOKEN_RE='^[A-Za-z0-9_-]{20,}$'

nros_runner_token_ok() {
    [[ "$1" =~ $NROS_RUNNER_TOKEN_RE ]]
}

nros_mint_runner_token() {
    local script="$1" repo="$2" attempts="${3:-4}" i tok rc why
    for ((i = 1; i <= attempts; i++)); do
        rc=0
        tok="$(gh api -X POST "repos/${repo}/actions/runners/registration-token" \
                 --jq .token 2>/dev/null)" || rc=$?
        if [ "$rc" -eq 0 ] && nros_runner_token_ok "$tok"; then
            printf '%s\n' "$tok"
            return 0
        fi
        if [ "$rc" -ne 0 ]; then
            why="gh api exited $rc"
        else
            why="the value is not a token ($(printf '%s' "$tok" | wc -l | tr -d ' ') newline(s), ${#tok} chars)"
        fi
        echo "$script: registration-token mint $i/$attempts failed: $why" >&2
        [ "$i" -lt "$attempts" ] && sleep $((i * 20))
    done
    {
        echo "$script: could not mint a registration token for $repo."
        echo "  It needs ADMIN on the repo (an authenticated gh is not enough),"
        echo "  and the API must answer: try 'gh api repos/$repo --jq .permissions.admin'."
    } >&2
    return 1
}
