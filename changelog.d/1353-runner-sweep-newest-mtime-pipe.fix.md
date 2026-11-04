`runner-sweep` no longer prints dozens of `sort: write error` lines when it
ranks a large build directory for eviction. The newest-file lookup piped into
`sort | head -1`, and under a job step's ignored SIGPIPE `sort` reported the
closed pipe for every entry with thousands of files, which read like a full
disk (issue 1353 fixed the same noise in the eviction loop).
