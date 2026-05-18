// nros Service Client — calls example_interfaces/AddTwoInts every
// second and prints the response.
//
// Phase 23.4.3. Will not link until Phase 23.2 populates
// `arduino/nros/src/<arch>/libnanoros.a` for your ESP32 variant.
//
// Run a service server on the host before flashing — e.g.:
//   ros2 run examples_rclcpp_minimal_service service_main
// or:
//   ros2 run demo_nodes_cpp add_two_ints_server
// with the `rmw_zenoh` middleware selected. zenohd bridges this
// sketch's request through to the host node.

#include <nros_arduino.h>
#include <nros/init.h>
#include <nros/node.h>
#include <nros/client.h>
#include <example_interfaces/srv/add_two_ints.h>

// ─── User configuration ─────────────────────────────────────────────
static const char* WIFI_SSID = "YourSSID";
static const char* WIFI_PASS = "YourPassword";
static const char* ZENOH_LOCATOR = "tcp/192.168.1.100:7447";
static const char* SERVICE_NAME = "/add_two_ints";
// ────────────────────────────────────────────────────────────────────

nros_context_t ctx;
nros_node_t node;
nros_client_t client;

void setup() {
    Serial.begin(115200);
    delay(500);

    set_nanoros_wifi_transports(WIFI_SSID, WIFI_PASS, ZENOH_LOCATOR);

    NRCHECK(nros_init(&ctx));
    NRCHECK(nros_node_create(&node, &ctx, "add_two_ints_client"));
    NRCHECK(nros_client_create(&client, &node, SERVICE_NAME,
        NANO_ROS_SRV_TYPE_SUPPORT(example_interfaces, srv, AddTwoInts)));

    Serial.println("[client] ready");
}

void loop() {
    example_interfaces__srv__AddTwoInts_Request req;
    example_interfaces__srv__AddTwoInts_Response resp;
    req.a = 41;
    req.b = 1;

    int rc = nros_client_call(&client, &req, sizeof(req), &resp, sizeof(resp));
    if (rc == 0) {
        Serial.printf("[client] %ld + %ld = %ld\n",
                      (long)req.a, (long)req.b, (long)resp.sum);
    } else {
        Serial.printf("[client] call failed: %d\n", rc);
    }

    nros_spin_once(&ctx, 100);
    delay(1000);
}
