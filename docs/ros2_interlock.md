# Kirra ROS2 Safety Interlock

## Architecture

The Kirra ROS2 safety interlock sits between your navigation stack (Nav2, AI planners, or any autonomous path-following system) and your physical motor controllers. Every velocity command is submitted to the Kirra verifier for kinematic enforcement and fleet posture checking before it reaches hardware. The package provides three ROS2 nodes that work together:

```
Nav2 / AI Planner
       |
       | /cmd_vel_raw  (geometry_msgs/Twist)
       v
+-------------------------------------------+
|    cmd_vel_interceptor                    |
|                                           |
|  Twist -> ProposedVehicleCommand          |
|  POST /actuator/motion/command            |
|  <- EnforceAction (Allow/Clamp/Deny)     |
|  Enforced Twist reconstruction            |
+-------------------------------------------+
       |
       | /cmd_vel  (geometry_msgs/Twist)
       v
  Motor Controllers
```

Additional nodes run in parallel:

```
sensor_monitor ---- POST /fleet/diagnostics/report ---> Kirra
                 ^
    /scan, /imu/data, /odom, /camera/depth/image_raw

posture_subscriber <- SSE /system/posture/stream <- Kirra
                 |
                 +-> /kirra/fleet_posture  (String)
                 +-> /kirra/posture_events (String)
                 +-> /cmd_vel  ZERO (on LockedOut transition)
```

### Node Responsibilities

**cmd_vel_interceptor** — The critical path node. It intercepts every `geometry_msgs/Twist` on its input topic, converts it to an Kirra `ProposedVehicleCommand` using a bicycle-model approximation for the mecanum drive geometry, and POSTs it to `/actuator/motion/command`. Kirra returns an `EnforceAction` (`Allow`, `Clamp`, or `DenyBreach`) with optionally clamped velocity and steering values. The interceptor reconstructs a safe Twist preserving the original lateral motion ratios and publishes it to the output topic. On any failure (timeout, connection error, denied, unexpected HTTP status), it publishes a zero-velocity Twist and logs the action — fail-closed by default.

**sensor_monitor** — Monitors ROS2 sensor topics and continuously reports confidence scores to Kirra via `/fleet/diagnostics/report`. It tracks message freshness, frequency (Hz), and quality metrics (IMU covariance) for four sensors: lidar_front, depth_camera, imu_primary, and wheel_encoders. Kirra uses these reports to compute per-node trust states and fleet-wide posture via its DAG traversal algorithm. When sensors degrade or go silent, posture transitions propagate through the dependency graph and the interceptor starts receiving denials or restricted commands.

**posture_subscriber** — Maintains a live SSE (Server-Sent Events) connection to Kirra and republishes posture transitions as ROS2 topics. When posture transitions to `LockedOut`, it immediately publishes a zero-velocity emergency stop to the safe velocity topic. It reconnects automatically if the Kirra connection drops.

---

## Installation

### Prerequisites

- ROS2 Humble or Iron (tested on Humble)
- Python 3.10+
- `python3-requests` (`pip3 install requests` or `sudo apt install python3-requests`)
- Kirra verifier service running and accessible

### Build

```bash
cd ~/ros2_ws
colcon build --packages-select kirra_safety
source install/setup.bash
```

If you want to also build the C++ bridge package alongside:

```bash
colcon build --packages-select kirra_safety kirra_bridge_cpp
source install/setup.bash
```

### Environment Variables

The Kirra token should be set in the environment rather than hardcoded in config files:

```bash
export KIRRA_ADMIN_TOKEN="your-admin-token-here"
export KIRRA_URL="http://localhost:8090"   # optional, default is localhost:8090
```

---

## Topic Remapping for Nav2 Integration

Nav2's `controller_server` publishes velocity commands to `/cmd_vel` by default. To insert the Kirra interlock, you remap Nav2's output to a raw topic and configure the interceptor to consume from that raw topic and publish to `/cmd_vel` (which your motor drivers subscribe to).

The `kirra_with_robot.launch.py` file does this automatically:

```
nav2_bringup publishes to  /cmd_vel_raw   (configure via cmd_vel_topic launch arg)
cmd_vel_interceptor reads  /cmd_vel_raw   (input_topic parameter)
cmd_vel_interceptor writes /cmd_vel       (output_topic parameter)
motor drivers read         /cmd_vel
```

To configure Nav2 to publish to `/cmd_vel_raw`:

```yaml
# nav2_params.yaml
controller_server:
  ros__parameters:
    cmd_vel_topic: /cmd_vel_raw
```

Or pass it as a launch argument:

```bash
ros2 launch nav2_bringup navigation_launch.py cmd_vel_topic:=/cmd_vel_raw
```

---

## Fleet Node Registration

Before starting the robot, register the sensor dependency graph with Kirra. This tells Kirra which nodes exist, how they depend on each other, and what constitutes a healthy fleet.

Run the registration script once (after Kirra is running):

```bash
KIRRA_ADMIN_TOKEN=your-token bash scripts/setup_ros2_fleet.sh
```

Or with a custom Kirra URL:

```bash
KIRRA_URL=http://192.168.1.100:8090 KIRRA_ADMIN_TOKEN=your-token bash scripts/setup_ros2_fleet.sh
```

The script registers the following dependency graph:

```
lidar_front ----+
                +--> perception_fusion ----+
depth_camera ---+                          |
                                           +--> navigation_stack --> motor_controller
imu_primary ----+                          |
                +--> odometry_fusion ------+
wheel_encoders -+
```

This graph means:
- If `lidar_front` fails, `perception_fusion` becomes Untrusted, which cascades to `navigation_stack` (Degraded), and finally `motor_controller` is restricted.
- If two or more critical sensors fail simultaneously, the fleet may transition to `LockedOut`, triggering an emergency stop.

After registration, replace the placeholder TPM keys with real AK public keys if you are using hardware attestation:

```bash
# Edit the script and replace:
# "ak_public_pem": "PLACEHOLDER_PEM_REPLACE_WITH_REAL_KEY"
# with the actual PEM-encoded AK public key from your TPM.
```

---

## Hiwonder ROSOrin Quick-Start

The ROSOrin is a mecanum-wheeled robot with a wheelbase of 0.2 m and a max speed of 1.8 m/s. All three motion axes are active (linear.x forward, linear.y lateral strafing, angular.z rotation).

### Step 1: Start Kirra

```bash
KIRRA_ADMIN_TOKEN=your-token ./target/release/kirra_verifier_service
```

### Step 2: Register the fleet graph

```bash
KIRRA_ADMIN_TOKEN=your-token bash scripts/setup_ros2_fleet.sh
```

### Step 3: Build and launch the interlock

```bash
cd ~/ros2_ws
colcon build --packages-select kirra_safety
source install/setup.bash

ros2 launch kirra_safety kirra_with_robot.launch.py \
  kirra_url:=http://localhost:8090 \
  kirra_token:=$KIRRA_ADMIN_TOKEN
```

### Step 4: Verify operation

```bash
# Monitor the safe velocity output
ros2 topic echo /cmd_vel

# Monitor Kirra enforcement decisions
ros2 topic echo /kirra/enforcement_action

# Monitor fleet posture
ros2 topic echo /kirra/fleet_posture

# Publish a test velocity (should appear on /cmd_vel after Kirra approval)
ros2 topic pub /cmd_vel_raw geometry_msgs/msg/Twist \
  "{linear: {x: 0.3, y: 0.0, z: 0.0}, angular: {x: 0.0, y: 0.0, z: 0.0}}"
```

### Step 5: Test the safety interlock

Publish a velocity that exceeds the 1.8 m/s limit:

```bash
ros2 topic pub /cmd_vel_raw geometry_msgs/msg/Twist \
  "{linear: {x: 3.0, y: 0.0, z: 0.0}, angular: {x: 0.0, y: 0.0, z: 0.0}}"
```

The interceptor should clamp the output to 1.8 m/s and log a warning. The `/kirra/enforcement_action` topic will show `Clamp:v=1.80`.

---

## Tuning Parameters for Different Robots

All parameters are configured in `config/kirra_params.yaml` or overridden via launch arguments.

### cmd_vel_interceptor parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| `kirra_url` | `http://localhost:8090` | Kirra verifier base URL |
| `kirra_token` | `""` | Bearer token (set via env or launch arg) |
| `kirra_client_id` | `ros2-interceptor-01` | Client ID for identity-gated routes |
| `wheelbase_m` | `0.2` | Robot wheelbase for bicycle model (ROSOrin: 0.2 m) |
| `max_speed_mps` | `1.8` | Maximum speed (ROSOrin: 1.8 m/s) |
| `input_topic` | `/cmd_vel` | Velocity input from planner |
| `output_topic` | `/cmd_vel_safe` | Enforced velocity to motors |
| `timeout_ms` | `50` | Kirra API call timeout in ms |
| `fallback_on_timeout` | `stop` | `stop` (fail-closed) or `passthrough` |

For slower robots (e.g., warehouse AMRs at 0.5 m/s):

```yaml
cmd_vel_interceptor:
  ros__parameters:
    wheelbase_m: 0.5
    max_speed_mps: 0.5
    timeout_ms: 100
```

For faster outdoor robots where a brief stop is unsafe (tuned passthrough mode):

```yaml
cmd_vel_interceptor:
  ros__parameters:
    max_speed_mps: 5.0
    timeout_ms: 30
    fallback_on_timeout: "passthrough"  # Allow if Kirra unreachable
```

### sensor_monitor parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| `report_interval_ms` | `200` | Health report posting frequency |
| `lidar_min_hz` | `5.0` | LiDAR Hz below which confidence degrades |
| `imu_covariance_threshold` | `0.1` | Angular velocity covariance fault threshold |
| `odometry_stale_ms` | `500` | Odometry max age before confidence drops to 0 |

For a high-speed robot where IMU freshness is critical:

```yaml
sensor_monitor:
  ros__parameters:
    report_interval_ms: 100
    odometry_stale_ms: 200
    imu_covariance_threshold: 0.05
```

---

## Troubleshooting

### Kirra is unreachable

By default the interceptor is fail-closed: if it cannot reach Kirra (connection refused, timeout), it publishes a zero-velocity stop and logs `CONNECTION_ERROR:STOP`. The robot will stop until Kirra becomes reachable again.

To change this behavior to allow passthrough when Kirra is unreachable:

```yaml
cmd_vel_interceptor:
  ros__parameters:
    fallback_on_timeout: "passthrough"
```

Use passthrough only in controlled environments where stopping the robot is more dangerous than allowing unverified commands.

### Fleet posture is Degraded

When posture is `Degraded`, Kirra allows `ReadTelemetry` operations but restricts `WriteState` mutations. The interceptor will receive HTTP 403 or a `DenyBreach` action for motion commands. Check which sensors are reporting low confidence:

```bash
# Check the posture event stream
ros2 topic echo /kirra/posture_events

# Or query Kirra directly
curl -H "Authorization: Bearer $KIRRA_ADMIN_TOKEN" http://localhost:8090/fleet/posture
```

### Fleet posture is LockedOut — Emergency Stop

When posture is `LockedOut`:
1. `posture_subscriber` immediately publishes a zero-velocity twist to the safe velocity topic.
2. `cmd_vel_interceptor` will receive HTTP 503 on all subsequent commands and continue publishing zeros.
3. The robot remains stopped until posture recovers to `Nominal`.

Recovery requires a consecutive streak of healthy sensor reports (5 reports within a 10-second window per the Kirra recovery hysteresis algorithm). Once sensors recover, posture will transition back to `Degraded` and then `Nominal` automatically.

### High enforcement latency

The interceptor has a 50ms timeout by default. If your Kirra instance is on a remote host, you may see increased latency. Options:

1. Increase `timeout_ms` to 100-200ms (and accept slower response to commands).
2. Run Kirra on the robot's onboard computer (`localhost`) to minimize round-trip time.
3. Pre-authorize a velocity range and use the posture subscriber only to gate emergency stops, reducing per-command HTTP calls.

### posture_subscriber cannot connect to SSE stream

The SSE stream (`GET /system/posture/stream`) is identity-gated: it requires both a Bearer token and the `x-kirra-client-id` header. Verify:

1. `KIRRA_TRUSTED_INGRESS_MODE=true` is set in the Kirra environment if identity gating is enabled.
2. The `kirra_token` and `kirra_client_id` parameters are correctly set.

The bridge reconnects automatically every 5 seconds on failure and logs the reconnection attempts.

---

## LiDAR Cover Demo Scenario

This scenario demonstrates the end-to-end safety response to sensor occlusion:

1. **Start state**: Fleet posture is `Nominal`. Robot drives at 0.5 m/s. LiDAR reports confidence 1.0 at 10 Hz.

2. **Cover the LiDAR**: Place an object over the RPLIDAR A2 sensor. The `/scan` topic goes silent.

3. **Sensor monitor detects staleness**: After 2 seconds of silence, `_lidar_confidence()` returns 0.0. The monitor posts a report: `{"node_id": "lidar_front", "confidence_score": 0.0, "hardware_fault": false}`.

4. **Kirra processes the report**: Kirra marks `lidar_front` as Untrusted. The DAG traversal propagates: `perception_fusion` becomes Degraded, `navigation_stack` becomes Degraded, `motor_controller` becomes Degraded. Fleet posture transitions to `Degraded`.

5. **Posture subscriber receives the event**: The SSE stream delivers a `{"posture": "Degraded", "reason": "..."}` event. The subscriber publishes `"Degraded"` on `/kirra/fleet_posture`.

6. **Interceptor receives HTTP 403**: The next cmd_vel command to Kirra returns a denial. The interceptor publishes a zero-velocity stop and logs `BLOCKED:HTTP_403`.

7. **Robot slows and stops**: The motor controllers receive zero velocity. The robot stops.

8. **Uncover the LiDAR**: The `/scan` topic resumes. The sensor monitor starts receiving messages and rebuilds the Hz estimate.

9. **Recovery hysteresis**: After 5 consecutive healthy reports within 10 seconds, Kirra marks `lidar_front` Trusted again. The DAG traversal clears the cascade. Fleet posture transitions back to `Nominal`.

10. **Commands resume**: The interceptor receives `Allow` responses again. The robot can resume autonomous motion.

The full transition takes approximately 10-15 seconds from sensor recovery to posture restoration, governed by the Kirra `AV_RECOVERY_STREAK_THRESHOLD` (5 reports) and `AV_RECOVERY_WINDOW_MS` (10 seconds) constants.

---

## Custom Message Types

The package defines two custom ROS2 message types in the `msg/` directory:

**KirraPosture.msg** — Carries a posture snapshot with the list of blocked nodes, a Unix timestamp, and the monotonic generation counter from Kirra. Subscribe to `/kirra/fleet_posture` using this message type when you need structured posture data (the simple String publisher uses the posture label only).

**SensorHealth.msg** — Mirrors the Kirra `/fleet/diagnostics/report` payload as a ROS2 message. Can be used to republish sensor health data on the ROS2 graph for visualization in tools like rqt.

To use these in other packages after building:

```python
from kirra_safety.msg import KirraPosture, SensorHealth
```

---

## Security Considerations

- Never hardcode the `KIRRA_ADMIN_TOKEN` in config files or launch files. Use environment variables.
- The interceptor includes the admin token in every HTTP request. Use TLS (`https://`) in production to prevent token interception.
- The `fallback_on_timeout: passthrough` mode bypasses Kirra enforcement when the service is unreachable. Use it only in environments where stopping is more dangerous than operating without enforcement, and ensure Kirra is deployed with high availability.
- The posture subscriber SSE thread runs as a daemon thread. If the main ROS2 node shuts down, the thread exits cleanly because `_running` is set to `False` in `destroy_node()`.
