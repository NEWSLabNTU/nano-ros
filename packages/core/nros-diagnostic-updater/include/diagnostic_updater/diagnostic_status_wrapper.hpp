// SPDX-License-Identifier: Apache-2.0
//
// diagnostic_status_wrapper.hpp — Phase 209.D
//
// Typed-view + builder over `diagnostic_msgs::msg::DiagnosticStatus`. Mirrors
// the upstream `diagnostic_updater::DiagnosticStatusWrapper` surface ported
// ROS 2 nodes use (summary + add(key, value) overloads + level constants).
//
// nano-ros expects `diagnostic_msgs` codegen to have run — generate it via
// `nano_ros_generate_interfaces(diagnostic_msgs DiagnosticStatus DiagnosticArray
// KeyValue LANGUAGE CPP)` (or `nros generate cpp diagnostic_msgs`) in your
// CMakeLists.txt before this header is included.

#ifndef NROS_DIAGNOSTIC_UPDATER_DIAGNOSTIC_STATUS_WRAPPER_HPP
#define NROS_DIAGNOSTIC_UPDATER_DIAGNOSTIC_STATUS_WRAPPER_HPP

#include <cstdint>
#include <cstdio>
#include <string>

#include <diagnostic_msgs/msg/diagnostic_status.hpp>
#include <diagnostic_msgs/msg/key_value.hpp>

namespace diagnostic_updater {

// Mirror of `diagnostic_msgs::msg::DiagnosticStatus::OK/WARN/ERROR/STALE`.
// Provided here too so source files that reference the wrapper before any
// generated message header compiles still pick up the canonical values.
constexpr uint8_t OK    = 0;
constexpr uint8_t WARN  = 1;
constexpr uint8_t ERROR = 2;
constexpr uint8_t STALE = 3;

class DiagnosticStatusWrapper : public ::diagnostic_msgs::msg::DiagnosticStatus {
public:
    DiagnosticStatusWrapper() {
        this->level   = OK;
        this->name    = "";
        this->message = "";
    }

    // --- summary -------------------------------------------------------------
    void summary(uint8_t lvl, const std::string& msg) {
        this->level   = lvl;
        this->message = msg;
    }

    void summary(const DiagnosticStatusWrapper& src) {
        this->level   = src.level;
        this->message = src.message;
    }

    void summaryf(uint8_t lvl, const char* fmt, ...) {
        char buf[256];
        va_list ap;
        va_start(ap, fmt);
        ::vsnprintf(buf, sizeof(buf), fmt, ap);
        va_end(ap);
        this->level   = lvl;
        this->message = buf;
    }

    void clearSummary() {
        this->level   = OK;
        this->message = "";
    }

    void mergeSummary(uint8_t lvl, const std::string& msg) {
        // Worst-of: keep the higher level; concatenate message.
        if (lvl > this->level) {
            this->level = lvl;
        }
        if (!this->message.empty() && !msg.empty()) {
            this->message += "; ";
        }
        this->message += msg;
    }

    // --- add(key, value) ----------------------------------------------------
    void add(const std::string& key, const std::string& value) {
        ::diagnostic_msgs::msg::KeyValue kv;
        kv.key   = key;
        kv.value = value;
        this->values.push_back(std::move(kv));
    }

    void add(const std::string& key, const char* value) {
        add(key, std::string(value ? value : ""));
    }

    template <typename T>
    void add(const std::string& key, T value) {
        // Default path covers integral + floating types via std::to_string.
        add(key, std::to_string(value));
    }

    void add(const std::string& key, bool value) {
        add(key, std::string(value ? "True" : "False"));
    }

    void addf(const std::string& key, const char* fmt, ...) {
        char buf[256];
        va_list ap;
        va_start(ap, fmt);
        ::vsnprintf(buf, sizeof(buf), fmt, ap);
        va_end(ap);
        add(key, std::string(buf));
    }

    void clear() {
        this->values.clear();
        clearSummary();
    }
};

}  // namespace diagnostic_updater

#endif  // NROS_DIAGNOSTIC_UPDATER_DIAGNOSTIC_STATUS_WRAPPER_HPP
