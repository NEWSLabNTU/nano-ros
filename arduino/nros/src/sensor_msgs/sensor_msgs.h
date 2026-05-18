#ifndef SENSOR_MSGS_H
#define SENSOR_MSGS_H

#include <nros/types.h>

// Dependencies
#include <builtin_interfaces/builtin_interfaces.h>
#include <std_msgs/std_msgs.h>
#include <geometry_msgs/geometry_msgs.h>

// Messages
#include "msg/sensor_msgs_msg_battery_state.h"
#include "msg/sensor_msgs_msg_camera_info.h"
#include "msg/sensor_msgs_msg_channel_float32.h"
#include "msg/sensor_msgs_msg_compressed_image.h"
#include "msg/sensor_msgs_msg_fluid_pressure.h"
#include "msg/sensor_msgs_msg_illuminance.h"
#include "msg/sensor_msgs_msg_image.h"
#include "msg/sensor_msgs_msg_imu.h"
#include "msg/sensor_msgs_msg_joint_state.h"
#include "msg/sensor_msgs_msg_joy.h"
#include "msg/sensor_msgs_msg_joy_feedback.h"
#include "msg/sensor_msgs_msg_joy_feedback_array.h"
#include "msg/sensor_msgs_msg_laser_echo.h"
#include "msg/sensor_msgs_msg_laser_scan.h"
#include "msg/sensor_msgs_msg_magnetic_field.h"
#include "msg/sensor_msgs_msg_multi_dofjoint_state.h"
#include "msg/sensor_msgs_msg_multi_echo_laser_scan.h"
#include "msg/sensor_msgs_msg_nav_sat_fix.h"
#include "msg/sensor_msgs_msg_nav_sat_status.h"
#include "msg/sensor_msgs_msg_point_cloud.h"
#include "msg/sensor_msgs_msg_point_cloud2.h"
#include "msg/sensor_msgs_msg_point_field.h"
#include "msg/sensor_msgs_msg_range.h"
#include "msg/sensor_msgs_msg_region_of_interest.h"
#include "msg/sensor_msgs_msg_relative_humidity.h"
#include "msg/sensor_msgs_msg_temperature.h"
#include "msg/sensor_msgs_msg_time_reference.h"
#include "msg/sensor_msgs_msg_set_camera_info_request.h"
#include "msg/sensor_msgs_msg_set_camera_info_response.h"

// Services
#include "srv/sensor_msgs_srv_set_camera_info.h"

#endif  // SENSOR_MSGS_H
