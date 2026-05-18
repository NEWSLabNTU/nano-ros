// nros Talker — publishes `std_msgs/Int32` on /chatter every second.
//
// Phase 23.4.1. Will not link until Phase 23.2 populates
// `arduino/nros/src/<arch>/libnanoros.a` for your ESP32 variant.

#include <nros_arduino.h>
#include <nros/init.h>
#include <nros/node.h>
#include <nros/publisher.h>
#include <std_msgs/msg/int32.h>

// ─── User configuration ─────────────────────────────────────────────
static const char* WIFI_SSID = "YourSSID";
static const char* WIFI_PASS = "YourPassword";
static const char* ZENOH_LOCATOR = "tcp/192.168.1.100:7447";
// ────────────────────────────────────────────────────────────────────

nros_context_t ctx;
nros_node_t node;
nros_publisher_t pub;
int count = 0;

void setup() {
    Serial.begin(115200);
    delay(500);

    set_nanoros_wifi_transports(WIFI_SSID, WIFI_PASS, ZENOH_LOCATOR);

    NRCHECK(nros_init(&ctx));
    NRCHECK(nros_node_create(&node, &ctx, "talker"));
    NRCHECK(nros_publisher_create(&pub, &node, "/chatter",
        NANO_ROS_MSG_TYPE_SUPPORT(std_msgs, msg, Int32)));

    Serial.println("[talker] ready, publishing every 1s");
}

void loop() {
    std_msgs__msg__Int32 msg = { .data = count++ };
    NRSOFTCHECK(nros_publish(&pub, &msg, sizeof(msg)));
    Serial.printf("[talker] published %d\n", msg.data);

    nros_spin_once(&ctx, 100);
    delay(1000);
}
