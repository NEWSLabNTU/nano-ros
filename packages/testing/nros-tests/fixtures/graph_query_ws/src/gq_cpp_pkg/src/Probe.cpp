#include "gq_cpp_pkg/Probe.hpp"

#include <cstdio>
#include <cstring>

namespace gq_cpp_pkg {

namespace {
constexpr int kStableProbes = 5;

struct Topics {
    size_t count = 0;
    char chatter_type[128] = "";
};

bool count_node(void* ctx, const char*, const char*, const char*) {
    ++*static_cast<size_t*>(ctx);
    return true;
}

bool count_topic(void* ctx, const char* name, const char* const* types, size_t n) {
    auto* t = static_cast<Topics*>(ctx);
    ++t->count;
    if (std::strcmp(name, "/chatter") == 0 && n > 0) {
        std::snprintf(t->chatter_type, sizeof(t->chatter_type), "%s", types[0]);
    }
    return true;
}

bool count_service(void* ctx, const char*, const char* const*, size_t) {
    ++*static_cast<size_t*>(ctx);
    return true;
}
} // namespace

Probe::Probe(::rclcpp::NodeHandle h) : ::rclcpp::NodeWithTimers<1>(h, "gq_cpp") {
    create_wall_timer_in<Probe, &Probe::on_timer>(100);
}

void Probe::on_timer() {
    if (done_) {
        return;
    }
    size_t publishers = 0, nodes = 0, services = 0;
    Topics topics;
    if (!count_publishers("/chatter", &publishers) || !get_node_names(count_node, &nodes) ||
        !get_topic_names_and_types(count_topic, &topics) ||
        !get_service_names_and_types(count_service, &services)) {
        return;
    }
    char answer[256];
    std::snprintf(answer, sizeof(answer),
                  "nodes=%zu topics=%zu services=%zu chatter_publishers=%zu chatter_type=%s",
                  nodes, topics.count, services, publishers, topics.chatter_type);
    if (std::strcmp(answer, last_) == 0) {
        ++stable_;
    } else {
        std::snprintf(last_, sizeof(last_), "%s", answer);
        stable_ = 0;
    }
    if (publishers == 0 || stable_ < kStableProbes) {
        return;
    }
    done_ = true;
    std::printf("GRAPH_QUERY lang=cpp %s\n", answer);
    std::fflush(stdout);
}

} // namespace gq_cpp_pkg
