# Test message for robot control types
# Tests: controller state, gripper control, joint trajectories

# Joint controller state with process value and errors
control_msgs/JointControllerState joint_controller_state

# Multi-DOF joint state for complex robots
control_msgs/MultiDOFStateStamped multi_dof_state

# Gripper command for gripper control
control_msgs/GripperCommand gripper_cmd

# Joint trajectory controller state
control_msgs/JointTrajectoryControllerState trajectory_controller_state

# PID controller state
control_msgs/PidState pid_state

# Single DOF state
control_msgs/SingleDOFState single_dof_state

# Steering controller status
control_msgs/SteeringControllerStatus steering_status

# Dynamic joint state for variable topology robots
control_msgs/DynamicJointState dynamic_joint_state

# Interface values for generic hardware interfaces
control_msgs/InterfaceValue[] interface_values
