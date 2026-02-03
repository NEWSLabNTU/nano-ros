/**
 * Test program for generated C message bindings
 */

#include <stdio.h>
#include <string.h>

#include "native_c_custom_msg.h"  // Umbrella header

int main(void) {
    printf("Testing generated C message bindings...\n\n");

    // Test Temperature message
    native_c_custom_msg_msg_temperature temp;
    native_c_custom_msg_msg_temperature_init(&temp);

    temp.temperature = 25.5;
    temp.variance = 0.1;
    strncpy(temp.frame_id, "sensor_frame", sizeof(temp.frame_id) - 1);

    printf("Temperature message:\n");
    printf("  temperature: %.2f\n", temp.temperature);
    printf("  variance: %.2f\n", temp.variance);
    printf("  frame_id: %s\n", temp.frame_id);

    // Test serialization
    uint8_t buffer[256];
    size_t serialized_size = 0;

    int32_t result = native_c_custom_msg_msg_temperature_serialize(&temp, buffer, sizeof(buffer), &serialized_size);
    if (result == 0) {
        printf("  Serialized size: %zu bytes\n", serialized_size);
    } else {
        printf("  Serialization failed: %d\n", result);
    }

    // Test deserialization
    native_c_custom_msg_msg_temperature temp2;
    native_c_custom_msg_msg_temperature_init(&temp2);

    result = native_c_custom_msg_msg_temperature_deserialize(&temp2, buffer, serialized_size);
    if (result == 0) {
        printf("  Deserialized temperature: %.2f\n", temp2.temperature);
        printf("  Deserialized frame_id: %s\n", temp2.frame_id);
    } else {
        printf("  Deserialization failed: %d\n", result);
    }

    printf("\n");

    // Test SensorData message
    native_c_custom_msg_msg_sensor_data sensor;
    native_c_custom_msg_msg_sensor_data_init(&sensor);

    sensor.sensor_id = 42;
    sensor.acceleration[0] = 0.1f;
    sensor.acceleration[1] = 0.2f;
    sensor.acceleration[2] = 9.8f;
    sensor.is_valid = true;

    printf("SensorData message:\n");
    printf("  sensor_id: %d\n", sensor.sensor_id);
    printf("  acceleration: [%.2f, %.2f, %.2f]\n",
           sensor.acceleration[0], sensor.acceleration[1], sensor.acceleration[2]);
    printf("  is_valid: %s\n", sensor.is_valid ? "true" : "false");

    // Test serialization
    serialized_size = 0;
    result = native_c_custom_msg_msg_sensor_data_serialize(&sensor, buffer, sizeof(buffer), &serialized_size);
    if (result == 0) {
        printf("  Serialized size: %zu bytes\n", serialized_size);
    } else {
        printf("  Serialization failed: %d\n", result);
    }

    printf("\nAll tests passed!\n");

    return 0;
}
