// nros Listener — subscribes to /chatter and prints each `std_msgs/Int32`.
//
// Phase 23.4.2. Will not link until Phase 23.2 populates
// `arduino/nros/src/<arch>/libnanoros.a` for your ESP32 variant.

#include <nros_arduino.h>
#include <std_msgs/std_msgs.h>

// ─── User configuration ─────────────────────────────────────────────
static const char* WIFI_SSID = "YourSSID";
static const char* WIFI_PASS = "YourPassword";
static const char* ZENOH_LOCATOR = "tcp/192.168.1.100:7447";
// ────────────────────────────────────────────────────────────────────

nros_context_t ctx;
nros_node_t node;
nros_subscription_t sub;

static void on_chatter(const uint8_t* data, size_t len, void* /*user_data*/) {
    std_msgs_msg_int32 msg = {};
    int32_t rc = std_msgs_msg_int32_deserialize(&msg, data, len);
    if (rc != 0) {
        Serial.printf("[listener] decode error %ld (%u bytes)\n",
                      (long)rc, (unsigned)len);
        return;
    }
    Serial.printf("[listener] got %d\n", (int)msg.data);
}

void setup() {
    Serial.begin(115200);
    delay(500);

    set_nanoros_wifi_transports(WIFI_SSID, WIFI_PASS, ZENOH_LOCATOR);

    NRCHECK(nros_init(&ctx));
    NRCHECK(nros_node_create(&node, &ctx, "listener"));
    NRCHECK(nros_subscription_create(&sub, &node, "/chatter",
        std_msgs_msg_int32_get_type_support(),
        on_chatter, nullptr));

    Serial.println("[listener] ready");
}

void loop() {
    nros_spin_once(&ctx, 100);
}
