# Test message for MoveIt motion planning types
# Tests: complex nested trajectories, large arrays, planning scene structures

# Robot trajectory with deeply nested structure
# RobotTrajectory -> JointTrajectory -> JointTrajectoryPoint[]
moveit_msgs/RobotTrajectory planned_trajectory

# Planning scene for collision checking
moveit_msgs/PlanningScene planning_scene

# Collision objects in the environment
moveit_msgs/CollisionObject[] collision_objects

# Attached collision objects (objects held by robot)
moveit_msgs/AttachedCollisionObject[] attached_objects

# Robot state with joint positions
moveit_msgs/RobotState robot_state

# Motion plan request
moveit_msgs/MotionPlanRequest plan_request

# Constraints for motion planning
moveit_msgs/Constraints path_constraints
moveit_msgs/Constraints goal_constraints

# Workspace parameters
moveit_msgs/WorkspaceParameters workspace_bounds
