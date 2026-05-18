// nros Reconnection — publishes std_msgs/Int32 on /chatter and
// reconnects when WiFi or zenohd drops.
//
// Phase 23.4.4. Will not link until Phase 23.2 populates
// `arduino/nros/src/<arch>/libnanoros.a` for your ESP32 variant.
//
// Models micro-ROS's `micro-ros_reconnection_example.ino`:
// `nanoros_ping()` is the cheap health check; on failure the
// sketch tears down nros + WiFi state and re-runs setup-style
// bring-up before the next publish.

#include <nros_arduino.h>
#include <std_msgs/std_msgs.h>

// ─── User configuration ─────────────────────────────────────────────
static const char* WIFI_SSID = "YourSSID";
static const char* WIFI_PASS = "YourPassword";
static const char* ZENOH_LOCATOR = "tcp/192.168.1.100:7447";
static const uint32_t PING_TIMEOUT_MS = 200;
static const uint32_t PUBLISH_PERIOD_MS = 500;
// ────────────────────────────────────────────────────────────────────

enum AgentState { AGENT_DOWN, AGENT_UP };

nros_context_t ctx;
nros_node_t node;
nros_publisher_t pub;
int count = 0;
AgentState state = AGENT_DOWN;

static void bring_up() {
    set_nanoros_wifi_transports(WIFI_SSID, WIFI_PASS, ZENOH_LOCATOR);
    NRCHECK(nros_init(&ctx));
    NRCHECK(nros_node_create(&node, &ctx, "talker"));
    NRCHECK(nros_publisher_create(&pub, &node, "/chatter",
        std_msgs_msg_int32_get_type_support()));
    state = AGENT_UP;
    Serial.println("[reconnect] agent up");
}

static void tear_down() {
    NRSOFTCHECK(nros_publisher_destroy(&pub));
    NRSOFTCHECK(nros_node_destroy(&node));
    NRSOFTCHECK(nros_fini(&ctx));
    state = AGENT_DOWN;
    Serial.println("[reconnect] agent down");
}

void setup() {
    Serial.begin(115200);
    delay(500);
    bring_up();
}

void loop() {
    if (state == AGENT_UP) {
        if (!nanoros_ping(PING_TIMEOUT_MS)) {
            tear_down();
        }
    }
    if (state == AGENT_DOWN) {
        delay(1000);
        bring_up();
        return;
    }

    std_msgs_msg_int32 msg = { count++ };
    NRSOFTCHECK(std_msgs_msg_int32_publish(&pub, &msg));
    nros_spin_once(&ctx, 100);
    delay(PUBLISH_PERIOD_MS);
}
