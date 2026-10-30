/// issue 2148 — the C++ twin of `bins/service-info-client`.
///
/// Calls `example_interfaces/srv/AddTwoInts` on `/add_two_ints`
/// `SERVICE_INFO_CLIENT_CALLS` times (default 3) with `a = i, b = 10`, sent
/// with `send_request` and taken with `PollClient::take_response(response, request_header)`
/// — upstream `rclcpp::Client::take_response`. Prints per reply:
///
///   RESPONSE_INFO guid=<32 hex digits> seq=<n> sum=<sum>
///
/// (`nros_tests::output::RESPONSE_INFO_LOG_PREFIX`): `guid` / `seq` are the
/// header's. A header whose GUID
/// the backend did not report prints `RESPONSE_INFO ABSENT …` instead — never
/// a zero GUID standing in for "unknown". After the last call it prints
/// `RESPONSE_INFO done`.
///
/// A bin, not `examples/linux/cpp/service-client`: that example is the
/// portable future road, and a header read is a probe's question.

#include <stdio.h>
#include <stdlib.h>

#include <rclcpp/rclcpp.hpp>

#include "example_interfaces.hpp"

using AddTwoInts = example_interfaces::srv::AddTwoInts;

static int env_int(const char* key, int fallback) {
    const char* v = getenv(key);
    return v != nullptr ? atoi(v) : fallback;
}

static bool guid_reported(const rmw_request_id_t& h) {
    for (int i = 0; i < 16; ++i)
        if (h.writer_guid[i] != 0) return true;
    return false;
}

static int run(rclcpp::Node& node, int calls);

int main(int argc, char** argv) {
#ifdef _IOLBF
    setvbuf(stdout, nullptr, _IOLBF, 0);
#endif
    if (!rclcpp::init_in(argc, argv).ok()) {
        fprintf(stderr, "RESPONSE_INFO rclcpp::init failed\n");
        return 1;
    }
    rclcpp::Node node;
    if (!rclcpp::create_node(node, "add_two_ints_info_client").ok()) {
        fprintf(stderr, "RESPONSE_INFO create_node failed\n");
        return 1;
    }
    int rc = run(node, env_int("SERVICE_INFO_CLIENT_CALLS", 3));
    rclcpp::shutdown();
    return rc;
}

// Its own frame, so the client is destroyed BEFORE `rclcpp::shutdown()`
// closes the session it lives on.
static int run(rclcpp::Node& node, int calls) {
    rclcpp::PollClient<AddTwoInts> client;
    if (!node.create_client(client, "/add_two_ints").ok()) {
        fprintf(stderr, "RESPONSE_INFO create_client failed\n");
        return 1;
    }
    // A backend that cannot tell (Unsupported) sends blind.
    rclcpp::Result waited = client.wait_for_service(20000);
    if (!waited.ok() && waited.code() != rclcpp::ErrorCode::Unsupported) {
        fprintf(stderr, "RESPONSE_INFO no server on /add_two_ints within 20 s\n");
        return 1;
    }

    for (int i = 0; i < calls; ++i) {
        AddTwoInts::Request request;
        request.a = i;
        request.b = 10;
        // The future is left unpolled: `take_response` takes the reply itself,
        // and the header is what names the request it answers.
        auto sent = client.send_request(request);
        if (sent.is_consumed()) {
            fprintf(stderr, "RESPONSE_INFO call %d: send failed\n", i);
            return 1;
        }
        AddTwoInts::Response response;
        rmw_request_id_t header{};
        bool taken = false;
        for (int step = 0; step < 1000 && !taken; ++step) {
            (void)rclcpp::spin_once(10);
            rclcpp::Result r = client.take_response(response, header);
            if (r.ok()) {
                taken = true;
            } else if (r.code() != rclcpp::ErrorCode::TryAgain) {
                fprintf(stderr, "RESPONSE_INFO call %d: take_response failed (%d)\n", i, r.raw());
                return 1;
            }
        }
        if (!taken) {
            fprintf(stderr, "RESPONSE_INFO call %d: no reply within 10 s\n", i);
            return 1;
        }
        if (!guid_reported(header)) {
            printf("RESPONSE_INFO ABSENT -- the backend reported no request-writer GUID "
                   "(seq=%lld sum=%lld)\n",
                   static_cast<long long>(header.sequence_number),
                   static_cast<long long>(response.sum));
            continue;
        }
        char hex[33];
        for (int b = 0; b < 16; ++b)
            snprintf(hex + 2 * b, 3, "%02x",
                     static_cast<unsigned>(static_cast<uint8_t>(header.writer_guid[b])));
        printf("RESPONSE_INFO guid=%s seq=%lld sum=%lld\n", hex,
               static_cast<long long>(header.sequence_number),
               static_cast<long long>(response.sum));
    }
    printf("RESPONSE_INFO done\n");
    return 0;
}
