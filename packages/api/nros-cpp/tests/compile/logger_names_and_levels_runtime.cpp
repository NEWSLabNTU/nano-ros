// phase-417 W4.d — two nodes in ONE C++ image log under DISTINCT names, and
// `rclcpp::Logger::set_level` moves ONE of them.
//
// The C++ twin of `nros-c/tests/run/named_logger_levels.c`, and the same
// property `nros-log`'s `named_loggers_levels_and_throttle.rs` proves for Rust.
// Three languages, one property, one implementation underneath it: RFC-0019
// makes `nros_log::resolve_logger` the only resolve-or-create there is, and
// assertion (2) below is what keeps that true — it compares the handle the
// NODE accessor produced against the handle the NAMED lookup produced, so a
// second table on either side is a failure rather than a coincidence that
// holds until someone adds one.
//
// A RUN rather than a `static_assert`, for the reason phase-427 W5 measured:
// every shape of this defect compiles. The accessors existed, were declared,
// were called, and three of the four resolved through a lookup-only
// `get_logger` that answers the catch-all for any unregistered name — so every
// node in an image logged under `"nros"` and no record said which one wrote it.
// A header check cannot tell two handles from one.
//
// `set_level` is W4.d's new C++ surface (`log.hpp`), and its ledger row
// (`cpp:Logger::set_level`) read "A ported node can raise or lower a logger in
// rclcpp and cannot here." Assertions (3)-(5) are what closes it; (6) is the
// half that is easy to get wrong in the other direction — a threshold write on
// a Logger with no handle must REFUSE, not silently move the catch-all's level
// and with it every unnamed logger in the image.
//
// The RMW backend is the shared stub (`nros-c/tests/run/stub_rmw_backend.c`),
// linked in: a node needs an executor, an executor needs a session, and nothing
// here touches the wire.

#include "nros/executor.hpp"
#include "nros/log.hpp"
#include "nros/node.hpp"

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

// --- the capture sink --------------------------------------------------------

const size_t MAX_RECORDS = 32;

struct Record {
    nros_log_severity_t severity;
    char logger[64];
    char message[128];
};

Record g_records[MAX_RECORDS];
size_t g_record_count = 0;

void copy_bounded(char* dst, size_t dst_len, const char* src, size_t src_len) {
    size_t n = (src_len < dst_len - 1) ? src_len : dst_len - 1;
    if (src != nullptr && n > 0) {
        std::memcpy(dst, src, n);
    }
    dst[n] = '\0';
}

extern "C" void capture_sink(void* user_data, nros_log_severity_t severity, const char* logger_name,
                             size_t logger_name_len, const char* message, size_t message_len,
                             const char* file, size_t file_len, uint32_t line,
                             uint64_t timestamp_ns) {
    (void)user_data;
    (void)file;
    (void)file_len;
    (void)line;
    (void)timestamp_ns;
    if (g_record_count >= MAX_RECORDS) return;
    g_records[g_record_count].severity = severity;
    copy_bounded(g_records[g_record_count].logger, sizeof(g_records[0].logger), logger_name,
                 logger_name_len);
    copy_bounded(g_records[g_record_count].message, sizeof(g_records[0].message), message,
                 message_len);
    ++g_record_count;
}

/// Records carrying `needle` AND naming `logger`. Both halves, because a record
/// that arrived under the WRONG name is the defect, not a cosmetic flaw.
int records_from(const char* logger, const char* needle) {
    int n = 0;
    for (size_t i = 0; i < g_record_count; ++i) {
        if (std::strstr(g_records[i].message, needle) != nullptr &&
            std::strcmp(g_records[i].logger, logger) == 0) {
            ++n;
        }
    }
    return n;
}

/// Records carrying `needle` under ANY name — separates "filtered out" from
/// "emitted under the wrong logger", two bugs with one symptom at the call site.
int records_anywhere(const char* needle) {
    int n = 0;
    for (size_t i = 0; i < g_record_count; ++i) {
        if (std::strstr(g_records[i].message, needle) != nullptr) ++n;
    }
    return n;
}

} // namespace

int main() {
    nros_log_init();
    check(nros_log_add_sink(capture_sink, nullptr),
          "the capture sink must install — an uninstalled sink never fires, which is "
          "indistinguishable from a filter that dropped everything");

    nros::Executor exec;
    check(nros::Executor::create_with_rmw(exec, NROS_STUB_RMW_NAME, nullptr, 0, "w4d_cpp").ok(),
          "executor create on the stub backend");

    rclcpp::Node alpha;
    rclcpp::Node beta;
    check(exec.create_node(alpha, "w4d_cpp_alpha").ok(), "alpha node");
    check(exec.create_node(beta, "w4d_cpp_beta").ok(), "beta node");
    if (g_failures != 0) {
        std::fprintf(stderr, "logger_names_and_levels_runtime: %d setup failure(s)\n", g_failures);
        return 1;
    }

    rclcpp::Logger alpha_log = alpha.get_logger();
    rclcpp::Logger beta_log = beta.get_logger();

    // --- 1. two nodes, two loggers, neither the catch-all --------------------
    check(static_cast<const void*>(alpha_log) != nullptr &&
              static_cast<const void*>(beta_log) != nullptr,
          "both node loggers must carry a handle");
    check(static_cast<const void*>(alpha_log) != static_cast<const void*>(beta_log),
          "two nodes answered ONE logger handle — phase-427 W5's defect, and every record in "
          "the image would name the same logger");
    check(static_cast<const void*>(alpha_log) != nros_log_default_logger() &&
              static_cast<const void*>(beta_log) != nros_log_default_logger(),
          "a node logger must not BE the catch-all: its threshold is shared with every "
          "unnamed logger, so moving it moves everyone");
    check(std::strcmp(alpha_log.get_name(), "w4d_cpp_alpha") == 0,
          "alpha's Logger must carry alpha's name");
    check(std::strcmp(beta_log.get_name(), "w4d_cpp_beta") == 0,
          "beta's Logger must carry beta's name");

    // --- 2. ONE resolve-or-create helper behind both spellings ---------------
    check(static_cast<const void*>(rclcpp::get_logger("w4d_cpp_alpha")) ==
              static_cast<const void*>(alpha_log),
          "rclcpp::get_logger(\"w4d_cpp_alpha\") answered a DIFFERENT handle than "
          "Node::get_logger() did — two tables, so a level set through one spelling is not "
          "the level the other filters on");
    check(static_cast<const void*>(rclcpp::get_logger("w4d_cpp_beta")) ==
              static_cast<const void*>(beta_log),
          "same, for beta: the node accessor and the named lookup must reach one helper");

    // --- 3. set_level moves ONE logger, measured at the sink -----------------
    check(alpha_log.set_level(rclcpp::Logger::Level::Error).ok(), "quiet alpha to ERROR");
    check(beta_log.set_level(rclcpp::Logger::Level::Info).ok(), "hold beta at INFO");

    RCLCPP_INFO(alpha_log, "w4d_cpp_marker_alpha_info");
    RCLCPP_INFO(beta_log, "w4d_cpp_marker_beta_info");
    RCLCPP_ERROR(alpha_log, "w4d_cpp_marker_alpha_error");

    check(records_anywhere("w4d_cpp_marker_alpha_info") == 0,
          "an INFO record on a logger set to ERROR reached a sink");
    check(records_from("w4d_cpp_beta", "w4d_cpp_marker_beta_info") == 1,
          "beta's INFO must survive under BETA's name — quieting alpha moved its sibling");
    check(records_from("w4d_cpp_alpha", "w4d_cpp_marker_alpha_error") == 1,
          "alpha's ERROR must arrive under ALPHA's name");

    // --- 4. get_level reads back what set_level wrote ------------------------
    check(alpha_log.get_level() == rclcpp::Logger::Level::Error,
          "alpha's level must read back as ERROR");
    check(beta_log.get_level() == rclcpp::Logger::Level::Info,
          "beta's level must read back as INFO");

    // --- 5. is_enabled agrees with the level ---------------------------------
    check(!alpha_log.is_enabled(rclcpp::Logger::Level::Info),
          "is_enabled(Info) must be false on a logger set to Error");
    check(alpha_log.is_enabled(rclcpp::Logger::Level::Error),
          "is_enabled(Error) must be true on a logger set to Error");
    check(beta_log.is_enabled(rclcpp::Logger::Level::Info),
          "is_enabled(Info) must be true on a logger still at Info");

    // --- 6. a handle-less Logger REFUSES a threshold write -------------------
    //
    // `rclcpp::Logger("orphan")` is the one-argument constructor: a name with no
    // handle behind it. Emitting through it is fine and lands in the catch-all
    // (`detail::log_handle`), because a record with no owner belongs there.
    // WRITING A THRESHOLD through it must not, and this is the assertion that
    // says so: redirecting it would move the level of every unnamed logger in
    // the image while reading, at the call site, as if it had moved "orphan"'s.
    const nros_log_severity_t before = nros_logger_get_level(nros_log_default_logger());
    rclcpp::Logger orphan("orphan");
    check(!orphan.set_level(rclcpp::Logger::Level::Fatal).ok(),
          "set_level on a Logger with no handle must REFUSE");
    check(nros_logger_get_level(nros_log_default_logger()) == before,
          "a refused set_level must not have moved the CATCH-ALL's level — that redirect is "
          "the silent failure this refusal exists to prevent");

    check(exec.shutdown().ok(), "shutdown");

    if (g_failures != 0) {
        std::fprintf(stderr, "logger_names_and_levels_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("logger_names_and_levels_runtime: ok (%d records captured)\n",
                static_cast<int>(g_record_count));
    return 0;
}
