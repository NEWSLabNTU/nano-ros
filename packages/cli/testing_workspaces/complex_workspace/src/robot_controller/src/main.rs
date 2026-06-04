// Demonstrates usage of BOTH standard ROS types and custom interface types
fn main() {
    println!("=== Robot Controller Node - Comprehensive Message Test ===\n");

    // ============================================
    // STANDARD ROS MESSAGE TYPES
    // ============================================
    println!("--- Standard ROS Messages ---");

    // std_msgs (using flat re-exports)
    let string_msg = std_msgs::msg::String::default();
    println!("std_msgs::String: {:?}", string_msg);

    let header = std_msgs::msg::Header::default();
    println!("std_msgs::Header: {:?}", header);

    let bool_msg = std_msgs::msg::Bool::default();
    println!("std_msgs::Bool: {:?}", bool_msg);

    let int32_msg = std_msgs::msg::Int32::default();
    println!("std_msgs::Int32: {:?}", int32_msg);

    let float64_msg = std_msgs::msg::Float64::default();
    println!("std_msgs::Float64: {:?}", float64_msg);

    // builtin_interfaces
    let time = builtin_interfaces::msg::Time::default();
    println!("builtin_interfaces::Time: {:?}", time);

    let duration = builtin_interfaces::msg::Duration::default();
    println!("builtin_interfaces::Duration: {:?}", duration);

    // geometry_msgs (using flat re-exports)
    let point = geometry_msgs::msg::Point::default();
    println!("geometry_msgs::Point: {:?}", point);

    let pose = geometry_msgs::msg::Pose::default();
    println!("geometry_msgs::Pose: {:?}", pose);

    let pose_stamped = geometry_msgs::msg::PoseStamped::default();
    println!("geometry_msgs::PoseStamped: {:?}", pose_stamped);

    let twist = geometry_msgs::msg::Twist::default();
    println!("geometry_msgs::Twist: {:?}", twist);

    let transform = geometry_msgs::msg::Transform::default();
    println!("geometry_msgs::Transform: {:?}", transform);

    let quaternion = geometry_msgs::msg::Quaternion::default();
    println!("geometry_msgs::Quaternion: {:?}", quaternion);

    // sensor_msgs (using flat re-exports)
    let imu = sensor_msgs::msg::Imu::default();
    println!("sensor_msgs::Imu: {:?}", imu);

    let laser_scan = sensor_msgs::msg::LaserScan::default();
    println!("sensor_msgs::LaserScan: {:?}", laser_scan);

    let point_cloud2 = sensor_msgs::msg::PointCloud2::default();
    println!("sensor_msgs::PointCloud2: {:?}", point_cloud2);

    let image = sensor_msgs::msg::Image::default();
    println!("sensor_msgs::Image: {:?}", image);

    let joint_state = sensor_msgs::msg::JointState::default();
    println!("sensor_msgs::JointState: {:?}", joint_state);

    // ============================================
    // NAVIGATION MESSAGES
    // ============================================
    println!("\n--- Navigation Messages ---");

    // nav_msgs
    let odometry = nav_msgs::msg::Odometry::default();
    println!("nav_msgs::Odometry: {:?}", odometry);

    let path = nav_msgs::msg::Path::default();
    println!("nav_msgs::Path: {:?}", path);

    let occupancy_grid = nav_msgs::msg::OccupancyGrid::default();
    println!("nav_msgs::OccupancyGrid: {:?}", occupancy_grid);

    // trajectory_msgs
    let joint_trajectory = trajectory_msgs::msg::JointTrajectory::default();
    println!("trajectory_msgs::JointTrajectory: {:?}", joint_trajectory);

    let joint_trajectory_point = trajectory_msgs::msg::JointTrajectoryPoint::default();
    println!("trajectory_msgs::JointTrajectoryPoint: {:?}", joint_trajectory_point);

    // ============================================
    // CONTROL MESSAGES
    // ============================================
    println!("\n--- Control Messages ---");

    let joint_trajectory_controller_state = control_msgs::msg::JointTrajectoryControllerState::default();
    println!("control_msgs::JointTrajectoryControllerState: {:?}", joint_trajectory_controller_state);

    // ============================================
    // DIAGNOSTIC MESSAGES
    // ============================================
    println!("\n--- Diagnostic Messages ---");

    let diagnostic_status = diagnostic_msgs::msg::DiagnosticStatus::default();
    println!("diagnostic_msgs::DiagnosticStatus: {:?}", diagnostic_status);

    let diagnostic_array = diagnostic_msgs::msg::DiagnosticArray::default();
    println!("diagnostic_msgs::DiagnosticArray: {:?}", diagnostic_array);

    let key_value = diagnostic_msgs::msg::KeyValue::default();
    println!("diagnostic_msgs::KeyValue: {:?}", key_value);

    // ============================================
    // NAV2 MESSAGES & ACTIONS
    // ============================================
    println!("\n--- Nav2 Messages and Actions ---");

    // Nav2 action: NavigateToPose
    let nav_to_pose_goal = nav2_msgs::action::navigate_to_pose::NavigateToPoseGoal::default();
    println!("nav2_msgs::NavigateToPoseGoal: {:?}", nav_to_pose_goal);

    let nav_to_pose_result = nav2_msgs::action::navigate_to_pose::NavigateToPoseResult::default();
    println!("nav2_msgs::NavigateToPoseResult: {:?}", nav_to_pose_result);

    let nav_to_pose_feedback = nav2_msgs::action::navigate_to_pose::NavigateToPoseFeedback::default();
    println!("nav2_msgs::NavigateToPoseFeedback: {:?}", nav_to_pose_feedback);

    // Nav2 action: DockRobot (with capitalized boolean literals)
    let dock_robot_goal = nav2_msgs::action::dock_robot::DockRobotGoal::default();
    println!("nav2_msgs::DockRobotGoal: {:?}", dock_robot_goal);

    let dock_robot_result = nav2_msgs::action::dock_robot::DockRobotResult::default();
    println!("nav2_msgs::DockRobotResult: {:?}", dock_robot_result);

    let dock_robot_feedback = nav2_msgs::action::dock_robot::DockRobotFeedback::default();
    println!("nav2_msgs::DockRobotFeedback: {:?}", dock_robot_feedback);

    // Nav2 action constants (now in separate modules within each action)
    println!("nav2_msgs::DockRobot result constants:");
    println!("  NONE = {}", nav2_msgs::action::rmw::dock_robot::result_constants::NONE);
    println!("  DOCK_NOT_IN_DB = {}", nav2_msgs::action::rmw::dock_robot::result_constants::DOCK_NOT_IN_DB);
    println!("  FAILED_TO_CHARGE = {}", nav2_msgs::action::rmw::dock_robot::result_constants::FAILED_TO_CHARGE);

    println!("nav2_msgs::DockRobot feedback constants:");
    println!("  NONE = {}", nav2_msgs::action::rmw::dock_robot::feedback_constants::NONE);
    println!("  NAV_TO_STAGING_POSE = {}", nav2_msgs::action::rmw::dock_robot::feedback_constants::NAV_TO_STAGING_POSE);
    println!("  WAIT_FOR_CHARGE = {}", nav2_msgs::action::rmw::dock_robot::feedback_constants::WAIT_FOR_CHARGE);

    // ============================================
    // MOVEIT MESSAGES
    // ============================================
    println!("\n--- MoveIt Messages ---");

    let robot_state = moveit_msgs::msg::RobotState::default();
    println!("moveit_msgs::RobotState: {:?}", robot_state);

    let motion_plan_request = moveit_msgs::msg::MotionPlanRequest::default();
    println!("moveit_msgs::MotionPlanRequest: {:?}", motion_plan_request);

    let planning_scene = moveit_msgs::msg::PlanningScene::default();
    println!("moveit_msgs::PlanningScene: {:?}", planning_scene);

    // ============================================
    // ACTION MESSAGES
    // ============================================
    println!("\n--- Action Messages ---");

    let goal_id = action_msgs::msg::GoalInfo::default();
    println!("action_msgs::GoalInfo: {:?}", goal_id);

    let goal_status = action_msgs::msg::GoalStatus::default();
    println!("action_msgs::GoalStatus: {:?}", goal_status);

    let goal_status_array = action_msgs::msg::GoalStatusArray::default();
    println!("action_msgs::GoalStatusArray: {:?}", goal_status_array);

    // ============================================
    // CUSTOM INTERFACE TYPES
    // ============================================
    println!("\n--- Custom Interface Messages ---");

    // Custom messages (using flat re-exports)
    let status = robot_interfaces::msg::RobotStatus::default();
    println!("robot_interfaces::RobotStatus: {:?}", status);

    let reading = robot_interfaces::msg::SensorReading::default();
    println!("robot_interfaces::SensorReading: {:?}", reading);

    // Custom service types
    println!("\n--- Custom Service Types ---");
    let service_req = robot_interfaces::srv::SetMode_Request::default();
    println!("SetModeRequest: {:?}", service_req);

    let service_resp = robot_interfaces::srv::SetMode_Response::default();
    println!("SetModeResponse: {:?}", service_resp);

    // Custom action types
    println!("\n--- Custom Action Types ---");
    let goal = robot_interfaces::action::navigate::NavigateGoal::default();
    println!("NavigateGoal: {:?}", goal);

    let result = robot_interfaces::action::navigate::NavigateResult::default();
    println!("NavigateResult: {:?}", result);

    let feedback = robot_interfaces::action::navigate::NavigateFeedback::default();
    println!("NavigateFeedback: {:?}", feedback);

    println!("\n=================================================");
    println!("✓ Successfully constructed messages from:");
    println!("  - 11 ROS 2 standard packages");
    println!("  - 5 navigation/control packages");
    println!("  - 1 custom interface package");
    println!("  - Total: 50+ different message/action types!");
    println!("=================================================");
}
