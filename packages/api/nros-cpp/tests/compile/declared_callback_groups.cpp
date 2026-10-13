// phase-459 W6 -- a node's callback groups and its registration's
// `CALLBACK_GROUPS` must name the same groups.
//
// RUNTIME probe. The refusal is decided at boot, inside the `NROS_COMPONENT`
// factory, so a `-fsyntax-only` sweep is green whatever it decides. This drives
// the two halves the factory composes, in the order it composes them:
//
//   * `admit_callback_group`, which `Node::create_callback_group` asks under the
//     scope the factory installs around the constructor;
//   * `first_uncreated_callback_group`, which the factory asks after it.
//
// and then the diagnostic both refusals print, which must name BOTH sides: the
// group and the declared list it disagrees with. The phase doc's negative case
// is the third block: `CALLBACK_GROUPS ctrl telem`, only `ctrl` created.
//
// Links no nano-ros archive: everything exercised is a header-only inline. The
// diagnostic reaches `nros_log`, whose two C entry points
// `nros_log_stderr_sink.hpp` defines (and which fails the probe if a record
// would not fit `nros_log`'s buffer, issue 1303).

#include <sys/wait.h>
#include <unistd.h>

#include <cstdio>
#include <cstdlib>
#include <cstring>

#include <nros/nros.hpp>

#include "nros_log_stderr_sink.hpp"

namespace {

namespace d = ::rclcpp::detail;

void expect(bool cond, const char* what) {
    if (!cond) {
        ::std::fprintf(stderr, "declared_callback_groups: FAILED: %s\n", what);
        ::std::abort();
    }
}

// What the factory does around a constructor that creates `created`.
struct Run {
    bool all_admitted;
    bool missing;
    char missing_name[32];
};

Run construct(const char* declared, const char* const* created, int n_created) {
    d::DeclaredCallbackGroups groups{declared, 0u};
    d::declared_callback_groups_scope() = &groups;
    Run r{true, false, {0}};
    for (int i = 0; i < n_created; ++i) {
        if (!d::admit_callback_group(created[i])) r.all_admitted = false;
    }
    d::declared_callback_groups_scope() = nullptr;
    const char* name = nullptr;
    int len = 0;
    r.missing = d::first_uncreated_callback_group(groups, &name, &len);
    if (r.missing) {
        expect(len > 0 && len < static_cast<int>(sizeof(r.missing_name)), "missing name length");
        ::std::memcpy(r.missing_name, name, static_cast<size_t>(len));
        r.missing_name[len] = '\0';
    }
    return r;
}

// The diagnostic, run in a child with its stderr piped back.
void diagnostic(const char* how, const char* name, int len, const char* list, char* out,
                size_t cap) {
    int fds[2];
    expect(::pipe(fds) == 0, "pipe");
    ::fflush(nullptr);
    const pid_t pid = ::fork();
    if (pid == 0) {
        ::close(fds[0]);
        ::dup2(fds[1], 2);
        ::close(fds[1]);
        d::report_callback_group_mismatch("subnode", how, name, len, list);
        ::_exit(0);
    }
    ::close(fds[1]);
    size_t used = 0;
    for (;;) {
        const ::ssize_t n = ::read(fds[0], out + used, cap - 1 - used);
        if (n <= 0) break;
        used += static_cast<size_t>(n);
        if (used >= cap - 1) break;
    }
    out[used] = '\0';
    ::close(fds[0]);
    int status = 0;
    expect(::waitpid(pid, &status, 0) == pid, "waitpid");
}

} // namespace

int main() {
    // The list spelling `nano_ros_node_register` emits.
    expect(d::declared_callback_group_count("|ctrl|telem|") == 2, "count of two");
    expect(d::declared_callback_group_index("|ctrl|telem|", "telem") == 1, "index of telem");
    expect(d::declared_callback_group_index("|ctrl|telem|", "tel") == -1, "a PREFIX is not a name");
    expect(d::declared_callback_group_index("|ctrl|telem|", "telemetry") == -1,
           "a longer name is not a name");

    const char* both[] = {"ctrl", "telem"};
    const char* ctrl_only[] = {"ctrl"};
    const char* stray[] = {"ctrl", "telem", "aux"};

    // (1) Code and keyword agree: realtime-cpp's subnode, unchanged.
    {
        const Run r = construct("|ctrl|telem|", both, 2);
        expect(r.all_admitted && !r.missing, "declared ctrl telem, created both: accepted");
    }
    // (2) Node-level placement: ONE declared group and none created is the
    // island's case, and is allowed.
    {
        const Run r = construct("|main|", nullptr, 0);
        expect(r.all_admitted && !r.missing, "one declared group, none created: accepted");
    }
    // (3) The phase's negative case: CALLBACK_GROUPS ctrl telem, only ctrl.
    {
        const Run r = construct("|ctrl|telem|", ctrl_only, 1);
        expect(r.all_admitted, "ctrl is declared");
        expect(r.missing && ::std::strcmp(r.missing_name, "telem") == 0,
               "declared telem never created: refused, naming telem");
    }
    // (4) A group the code made that the keyword never declared.
    {
        const Run r = construct("|ctrl|telem|", stray, 3);
        expect(!r.all_admitted, "aux is not declared: refused");
    }
    // (5) The registration said nothing: nothing refuses (no list, no scope).
    {
        const Run r = construct(nullptr, stray, 3);
        expect(r.all_admitted && !r.missing, "no CALLBACK_GROUPS: nothing checked");
        expect(d::admit_callback_group("anything"), "outside a factory scope: admitted");
    }
    // (6) The no-group name means "node default" and is always admitted.
    {
        d::DeclaredCallbackGroups g{"|ctrl|", 0u};
        d::declared_callback_groups_scope() = &g;
        expect(d::admit_callback_group(nullptr) && d::admit_callback_group(""),
               "the node-default name is admitted");
        d::declared_callback_groups_scope() = nullptr;
    }

    // The diagnostic names BOTH sides, for each refusal, and fits nros_log.
    {
        char out[2048];
        diagnostic("declared", "telem|", 5, "|ctrl|telem|", out, sizeof(out));
        expect(::std::strstr(out, "\"telem\"") != nullptr, "names the group, cut at its length");
        expect(::std::strstr(out, "|ctrl|telem|") != nullptr, "names the declared list");
        expect(::std::strstr(out, "never created") != nullptr, "says which way they disagree");
        expect(::std::strstr(out, "NROS_LOG_STDERR_SINK_OVERSIZE") == nullptr,
               "the record fits nros_log's buffer");

        diagnostic("created", "aux", 3, "|ctrl|telem|", out, sizeof(out));
        expect(::std::strstr(out, "\"aux\"") != nullptr, "names the undeclared group");
        expect(::std::strstr(out, "|ctrl|telem|") != nullptr, "names the declared list");
        expect(::std::strstr(out, "NROS_LOG_STDERR_SINK_OVERSIZE") == nullptr,
               "the record fits nros_log's buffer");
    }

    ::std::printf("declared_callback_groups: OK\n");
    return 0;
}
