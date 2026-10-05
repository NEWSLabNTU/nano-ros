// `rclcpp::init(argc, argv)` and `nros::init_with_launch*(argc, argv)` HONOUR
// `--ros-args` — the C++ half of phase-467 Row 11.
//
// RUNTIME probe, because every property here is an ORDER or a side effect, and
// `-fsyntax-only` is green for all of them:
//
//   1. a refused vector is refused BEFORE a session opens — the validate call
//      (NULL handle) comes first and `nros_cpp_init_rmw` is never reached;
//   2. an accepted vector is installed into the OPEN executor, after init;
//   3. a vector with no `--ros-args` never reaches the parser at all (what
//      every RTOS board passes is `(0, NULL)`);
//   4. a rule that does not fit closes the session it opened, rather than
//      leaving it running without the remap;
//   5. `rclcpp::init(argc, argv)` ABORTS on a refusal (its upstream shape
//      returns `void`, so there is nowhere else to put it), and
//      `init_with_launch_auto` RETURNS it.
//
// The parse itself is `nros_node::ros_args`'s, tested in Rust; here the five
// FFI entry points the headers reach are DEFINED BELOW as recording stubs, so
// no nano-ros archive is linked and no function symbol is unresolved (the
// loader refuses a binary with one — see publisher_publish_guards_initialized).

#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <initializer_list>
#include <sys/wait.h>
#include <unistd.h>

#include <nros/nros.hpp>

namespace {

enum class Ev { Validate, Init, Install, Fini };
Ev g_events[16];
int g_n = 0;
bool g_live = false;
bool g_install_full = false;

void record(Ev e) {
    if (g_n < 16) {
        g_events[g_n++] = e;
    }
}

void reset() {
    g_n = 0;
    g_live = false;
    g_install_full = false;
}

int failures = 0;

void check(bool cond, const char* what) {
    std::printf("  %s %s\n", cond ? "ok  " : "FAIL", what);
    if (!cond) {
        ++failures;
    }
}

bool events_are(std::initializer_list<Ev> want) {
    if (static_cast<int>(want.size()) != g_n) {
        return false;
    }
    int i = 0;
    for (Ev e : want) {
        if (g_events[i++] != e) {
            return false;
        }
    }
    return true;
}

} // namespace

extern "C" {
nros_cpp_ret_t nros_cpp_executor_storage_check(const void*, size_t) {
    return 0;
}
nros_cpp_ret_t nros_cpp_init_rmw(const char*, const char*, uint8_t, const char*, const char*,
                                 void*) {
    record(Ev::Init);
    g_live = true;
    return 0;
}
bool nros_cpp_context_is_live(const void*) {
    return g_live;
}
nros_cpp_ret_t nros_cpp_fini(void*) {
    record(Ev::Fini);
    g_live = false;
    return 0;
}
// Refuses `-p` like the real parser, so the headers' handling of a refusal is
// what is under test, not a stub's opinion of the grammar.
nros_cpp_ret_t nros_cpp_install_argv_remaps(void* handle, int argc, const char* const* argv,
                                            char* why, size_t why_len) {
    record(handle == nullptr ? Ev::Validate : Ev::Install);
    for (int i = 0; i < argc; ++i) {
        if (std::strcmp(argv[i], "-p") == 0) {
            std::snprintf(why, why_len, "`-p` is refused");
            return -3;
        }
    }
    if (handle != nullptr && g_install_full) {
        std::snprintf(why, why_len, "does not fit");
        return -6;
    }
    return 0;
}
}

int main() {
    static char prog[] = "prog", ros[] = "--ros-args", r[] = "-r", rule[] = "a:=b", p[] = "-p",
                param[] = "x:=1";
    char* remap_argv[] = {prog, ros, r, rule};
    char* param_argv[] = {prog, ros, p, param};
    char* plain_argv[] = {prog};

    std::printf("init_honours_ros_args:\n");

    reset();
    nros::Result res = nros::init_with_launch_auto(4, remap_argv);
    check(res.ok(), "init_with_launch_auto accepts `-r a:=b`");
    check(events_are({Ev::Validate, Ev::Init, Ev::Install}),
          "  ...validated BEFORE init, installed into the open executor AFTER it");

    reset();
    res = nros::init_with_launch_auto(4, param_argv);
    check(!res.ok() && res.raw() == -3, "init_with_launch_auto RETURNS the `-p` refusal");
    check(events_are({Ev::Validate}), "  ...and never opened a session");

    reset();
    res = nros::init_with_launch_auto(1, plain_argv);
    check(res.ok() && events_are({Ev::Init}),
          "no `--ros-args`: the parser is never reached, init proceeds");

    reset();
    res = nros::init_with_launch_auto(0, nullptr);
    check(res.ok() && events_are({Ev::Init}), "(0, NULL) — what RTOS boards pass — is a no-op");

    reset();
    g_install_full = true;
    res = nros::init_with_launch_auto(4, remap_argv);
    check(!res.ok() && res.raw() == -6, "a rule that does not fit is RETURNED");
    check(events_are({Ev::Validate, Ev::Init, Ev::Install, Ev::Fini}) && !g_live,
          "  ...and the session it opened is closed again");

    reset();
    rclcpp::init(4, remap_argv);
    check(events_are({Ev::Validate, Ev::Init, Ev::Install}),
          "rclcpp::init(argc, argv) installs `-r` the same way");

    // A refusal must ABORT before init. Forked, because abort is the property.
    reset();
    std::fflush(stdout);
    pid_t pid = fork();
    if (pid == 0) {
        rclcpp::init(4, param_argv);
        // Reaching here means it did not abort. Report whether init ran.
        std::_Exit(g_live ? 3 : 2);
    }
    int status = 0;
    waitpid(pid, &status, 0);
    check(WIFSIGNALED(status) && WTERMSIG(status) == SIGABRT,
          "rclcpp::init(argc, argv) ABORTS on a `-p` refusal");

    if (failures != 0) {
        std::printf("init_honours_ros_args: %d FAILED\n", failures);
        return 1;
    }
    std::printf("init_honours_ros_args: OK\n");
    return 0;
}
