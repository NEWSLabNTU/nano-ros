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
        std::fprintf(stderr, "param_store_full_runtime: %d failure(s)\n  line: %s\n", g_failures,
                     g_full_line);
        return 1;
    }
    std::printf("param_store_full_runtime: a full store refuses the boot, naming its knob "
                "(%d declared)\n",
                declared);
    return 0;
}
