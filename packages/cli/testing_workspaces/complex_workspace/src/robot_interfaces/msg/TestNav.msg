# Test message for navigation types
# Tests: 2D/3D grid data, particle filters, behavior trees, costmaps

# Costmap with 2D grid data
nav2_msgs/Costmap costmap

# Particle cloud for localization
nav2_msgs/ParticleCloud particle_cloud

# Particle hypothesis for localization
nav2_msgs/Particle[] particles

# Voxel grid for 3D obstacle representation
nav2_msgs/VoxelGrid voxel_grid

# Behavior tree status and events
nav2_msgs/BehaviorTreeLog behavior_tree_log
nav2_msgs/BehaviorTreeStatusChange[] bt_status_changes

# Speed limit information
nav2_msgs/SpeedLimit speed_limit

# Collision monitor state
nav2_msgs/CollisionMonitorState collision_state

# Costmap metadata
nav2_msgs/CostmapMetaData costmap_metadata

# Costmap filter information
nav2_msgs/CostmapFilterInfo filter_info
