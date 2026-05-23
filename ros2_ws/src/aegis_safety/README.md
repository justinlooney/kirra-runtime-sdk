# aegis_safety

ROS2 safety interlock package for the Aegis runtime legitimacy engine.

Provides three nodes that enforce kinematic contracts, monitor sensor health, and gate motion commands based on fleet posture:

- **cmd_vel_interceptor** — intercepts `/cmd_vel`, enforces via Aegis, publishes to `/cmd_vel_safe`
- **sensor_monitor** — reports LiDAR, IMU, camera, and odometry health to Aegis
- **posture_subscriber** — bridges the Aegis SSE posture stream to ROS2 topics and triggers emergency stops on `LockedOut` transitions

## Quick Start

```bash
cd ros2_ws
colcon build --packages-select aegis_safety
source install/setup.bash
ros2 launch aegis_safety aegis_with_robot.launch.py aegis_token:=$AEGIS_ADMIN_TOKEN
```

## Full Documentation

See [`docs/ros2_interlock.md`](../../../../docs/ros2_interlock.md) for:

- Full architecture diagram and node descriptions
- Installation steps
- Nav2 topic remapping guide
- Fleet node registration (setup_ros2_fleet.sh)
- Hiwonder ROSOrin quick-start
- Tuning parameters for different robots
- Troubleshooting (fail-closed behavior, posture recovery, SSE reconnection)
- The LiDAR cover demo scenario
