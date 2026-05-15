//! Network poll callback and global state for ESP32-C3 WiFi.

use esp_radio::wifi::WifiDevice;

nros_smoltcp::define_network_state!(NETWORK_STATE: WifiDevice, poll = smoltcp_network_poll);
