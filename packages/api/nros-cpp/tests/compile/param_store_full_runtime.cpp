// GitHub #4018 -- `declare_parameter<T>` past the parameter store's capacity
// is BOOT-FATAL, not a silent code default.
//
// The store is image-wide and sized at build time, so a node that declares
// more parameters than it holds has nowhere to put the rest. The facade used
// to answer the code default for every refusal but ALREADY_EXISTS, so a
// launch or YAML value for such a parameter was lost with no word, and a
// controller ran on its compiled gains. Now the runtime prints a line naming
// the node, the parameter, the capacity and the knob that sizes the store, and
// the facade latches `set_error` with `NROS_CPP_RET_FULL`, which the entry's
// post-construct `ok()` check halts on.
//
// A RUN, because the refusal is the store's answer at runtime. The store is
// the `param-services` archive's default size (no sizing descriptor reaches
// this build), and the probe declares until it is full rather than assuming a
// number. The line is read through a real `rclc_log_add_sink`.
//
// Built against an `nros-cpp` WITH `param-services` (`just check cpp`).

#define NROS_CPP_STD 1
#define NROS_SYSTEM_PARAM_SERVICES 1

// The header's own lines (the type-mismatch report) go to `NROS_LOG_SINK`,
// which a hosted build points at stderr. Route it here instead, so the probe
// can read the line it asserts on; everything still reaches stderr.
#include <cstdarg>
#include <cstdio>
#include <cstring>
#include <vector>
static char g_header_line[256];
static void capture_header_log(const char* level, const char* fmt, ...) {
    char buf[256];
    va_list ap;
    va_start(ap, fmt);
    std::vsnprintf(buf, sizeof buf, fmt, ap);
    va_end(ap);
    std::fprintf(stderr, "[%s] %s\n", level, buf);
    if (std::strstr(buf, "is declared double but") != nullptr) {
        std::snprintf(g_header_line, sizeof g_header_line, "%s", buf);
    }
}
#define NROS_LOG_SINK(level, file, line, ...) capture_header_log((level), __VA_ARGS__)

#include "rclcpp/rclcpp.hpp"

extern "C" {
#include "stub_rmw_backend.h"
}

#include <cstdio>
#include <cstring>

extern "C" void nros_app_register_backends(void);
extern "C" void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
}

namespace {

int g_failures = 0;

void check(bool cond, const char* what) {
    if (!cond) {
        ++g_failures;
        std::fprintf(stderr, "FAIL: %s\n", what);
    }
}

// The full-store report is THREE records (the line, then the knob, then the
// derivation), each short enough for the smallest log buffer. They are kept
// from the first "parameter store is full" record on, joined by a space.
char g_full_line[1024];
int g_records_kept = 0;

extern "C" void capture_sink(void*, rcl_log_severity_t, const char*, size_t, const char* message,
                             size_t message_len, const char*, size_t, uint32_t, uint64_t) {
    char buf[256];
    const size_t n = message_len < sizeof(buf) - 1 ? message_len : sizeof(buf) - 1;
    std::memcpy(buf, message, n);
    buf[n] = '\0';
    if (g_records_kept >= 3) {
        return; // the FIRST report is the one this probe is about
    }
    if (g_records_kept == 0 && std::strstr(buf, "parameter store is full") == nullptr) {
        return;
    }
    const size_t have = std::strlen(g_full_line);
    std::snprintf(g_full_line + have, sizeof(g_full_line) - have, "%s%s", have == 0 ? "" : " ",
                  buf);
    ++g_records_kept;
}

} // namespace

int main() {
    check(rclc_log_add_sink(&capture_sink, nullptr), "install the capture sink");
    nros_stub_rmw_set_accept_entities(true);
    rclcpp::Executor exec;
    check(rclcpp::Executor::create_with_rmw(exec, NROS_STUB_RMW_NAME, nullptr, 0, "i4018").ok(),
          "executor create on the stub backend");

    rclcpp::Node node;
    check(exec.create_node(node, "full_node").ok(), "create the node");
    check(node.ok(), "a fresh node is ok");

    // First, while the store has room: a value already in the store with another type than the code
    // declares (a YAML `kp: 1` read as a double) latches too, and the line says what to write
    // instead.
    // Seeded the way a launch `<param>` is: as text, typed by the same reading
    // `nros::main!` uses, and BEFORE the node exists -- a seed binds to the
    // next node the executor builds.
    check(nros_cpp_declare_node_param(exec.handle(), "typed_node", "kp", "1") == NROS_CPP_RET_OK,
          "seed kp from the launch text \"1\"");
    rclcpp::Node typed;
    check(exec.create_node(typed, "typed_node").ok(), "create the typed node");
    const double kp = typed.declare_parameter<double>("kp", 0.5);
    check(!typed.ok(), "a double declaration over an integer value latches the error");
    check(kp == 0.5, "the mismatched declaration still returns the code default");
    check(std::strstr(g_header_line, "\"kp\"") != nullptr, "the mismatch line names kp");
    check(std::strstr(g_header_line, "the integer 1;") != nullptr,
          "the mismatch line states the value the store holds");
    check(std::strstr(g_header_line, "write 1.0 in the YAML") != nullptr,
          "the mismatch line says what to write");

    // A launch list of doubles (`[40.0, 50.0, 60.0]` in a YAML `<param from>`)
    // arrives as a double array, so declaring it `std::vector<double>` passes
    // the type check above and reads back every element.
    check(nros_cpp_declare_node_param(exec.handle(), "list_node", "gains", "[40.0, 50.0, 60.0]") ==
              NROS_CPP_RET_OK,
          "seed gains from the launch list text");
    rclcpp::Node listed;
    check(exec.create_node(listed, "list_node").ok(), "create the list node");
    const std::vector<double> gains =
        listed.declare_parameter<std::vector<double>>("gains", std::vector<double>{});
    check(listed.ok(), "a double list declared as std::vector<double> is not refused");
    check(gains.size() == 3 && gains[0] == 40.0 && gains[1] == 50.0 && gains[2] == 60.0,
          "the double list reads back as [40.0, 50.0, 60.0]");

    // Fill the store. Far more than any default capacity, and the loop stops
    // at the first refusal, so the count never has to be known here.
    char name[32];
    int declared = 0;
    int64_t got = 0;
    for (int i = 0; i < 4096 && node.ok(); ++i) {
        std::snprintf(name, sizeof name, "p%d", i);
        got = node.declare_parameter<int64_t>(name, 7);
        if (node.ok()) {
            ++declared;
        }
    }
    check(declared > 0, "some parameters fit before the store filled");
    check(!node.ok(), "a declaration past the store's capacity latches the node's error");
    check(node.error_code() == NROS_CPP_RET_FULL, "the latched code is NROS_CPP_RET_FULL");
    const char* what = node.error_what();
    check(what != nullptr && std::strlen(what) <= 160,
          "the latched text fits one log record with the failure's prefix");
    check(what != nullptr && std::strstr(what, "max_parameters") != nullptr,
          "the latched text names `max_parameters`");
    check(what != nullptr && std::strstr(what, "[knobs.params]") != nullptr,
          "the latched text names `[knobs.params]`");
    check(what != nullptr && std::strstr(what, "NROS_MAX_PARAMETERS") != nullptr,
          "the latched text names NROS_MAX_PARAMETERS");
    check(what != nullptr && std::strstr(what, "CONFIG_NROS_MAX_PARAMETERS") != nullptr,
          "the latched text names CONFIG_NROS_MAX_PARAMETERS");
    check(got == 7, "the refused declaration still returns so the constructor can finish");
    check(!node.has_parameter(name), "the refused parameter is not declared");

    // The runtime's line: node, parameter, capacity, knob.
    check(g_full_line[0] != '\0', "a full-store line was logged");
    check(std::strstr(g_full_line, "/full_node") != nullptr, "the line names the node");
    check(std::strstr(g_full_line, name) != nullptr, "the line names the parameter");
    check(std::strstr(g_full_line, " slots, all nodes)") != nullptr,
          "the line states the capacity");
    check(std::strstr(g_full_line, "max_parameters") != nullptr, "the line names the knob");
    check(std::strstr(g_full_line, "NROS_MAX_PARAMETERS") != nullptr,
          "the line names the env spelling");
    check(std::strstr(g_full_line, "`params:`") != nullptr,
          "the line says the contract's `params:` derives the size");

    if (g_failures != 0) {
        std::fprintf(stderr,
                     "param_store_full_runtime: %d failure(s)\n  line: %s\n  mismatch: %s\n",
                     g_failures, g_full_line, g_header_line);
        return 1;
    }
    std::printf("param_store_full_runtime: a full store refuses the boot, naming its knob "
                "(%d declared)\n",
                declared);
    return 0;
}
