# kirra-ros2-adapter

ROS 2 adapter for the Kirra Governor. Implements the Option-B per-trajectory
wiring on top of Autoware's ROS 2 interfaces — see
`docs/safety/OCCY_131_OPTIONB_DESIGN.md` (KIRRA-OCCY-OPTIONB-001) for the
architecture this crate instantiates. Tracking issue: #131.

## Phase progression (this branch is Phase 2B)

| Phase | Scope | Status |
|-------|-------|--------|
| 1     | `AcceptedTrajectory` state machine + `AdaptorState` + `CorridorSource` trait + `MockCorridorSource` + r2r node skeleton (stubs). | landed (`occy-131-phase1-adapter`) |
| 2A    | Slow-loop validator: `validate_trajectory_slow` composes containment + per-pose kinematics + RSS into one verdict. `VehicleConfig` + `PerceivedObject`. Verified with `MockCorridorSource`. | landed (`occy-131-phase2a-validator`) |
| **2B**    | **`Lanelet2CorridorSource` — cxx wrapper around `lanelet2_core` + `boost::serialization` so the slow loop reads a real `LaneletMapBin.data` payload independently of the planner's `drivable_area`.** | **this branch (`occy-131-phase2b-lanelet2`)** |
| 3     | Fast-loop conformance check against the AcceptedTrajectory. | not started |
| 4     | MRC injection + wire `spawn_telemetry_watchdog` + CARLA demo. | not started |

## Build

### Default (no ROS deps) — the safety-kernel CI lane

```sh
cargo build -p kirra-ros2-adapter
cargo test  -p kirra-ros2-adapter
```

Builds only the state machine + corridor trait. No r2r, no Autoware
dependencies. This is what the workspace CI runs.

### With ROS 2 (`ros2` feature) — perception-governance build (NO C++)

```sh
source /opt/ros/jazzy/setup.bash      # or humble / kilted
# autoware_perception_msgs on AMENT_PREFIX_PATH (for the r2r bindings).
# No lanelet2 / boost / eigen needed for this build.

cargo build -p kirra-ros2-adapter --features ros2
```

Pulls `r2r = "=0.9.5"` (pinned) and compiles the **perception-governance
path** — `src/node.rs` (the r2r ROS 2 node), `parsing.rs`,
`perception_ingest.rs`. **No cxx, no C++, no lanelet2.** This is what the
sub-gate-1 mechanism validation (`tests/perception_mechanism_gate_ros2.rs`)
uses, and it builds on any machine with a sourced ROS 2 +
`autoware_perception_msgs`.

### With the corridor bridge (`lanelet2` feature) — map-aware build (C++)

```sh
source /opt/ros/jazzy/setup.bash
sudo apt install ros-${ROS_DISTRO}-lanelet2 libboost-serialization-dev libeigen3-dev

cargo build -p kirra-ros2-adapter --features ros2,lanelet2
```

`lanelet2` implies `ros2` and additionally pulls `cxx = "1.0"` +
`cxx-build = "1.0"` and compiles:
- `src/corridor/lanelet2_bridge.{rs,cpp,h}` — the cxx::bridge calling
  into `lanelet2_core` + `boost::serialization` to deserialize
  `LaneletMapBin.data`. Built via `cxx-build` from `build.rs`.
- `src/corridor/lanelet2.rs` — `Lanelet2CorridorSource` implementing
  the `CorridorSource` trait.

A binary built with `--features ros2` (no `lanelet2`) **hard-errors at
startup** if `--corridor-source lanelet2` / `--map-bin` is supplied — it
will not silently downgrade a requested real corridor to the mock.

**Prerequisites (the `lanelet2` feature only — the `ros2`-only build needs
none of these C++ deps):**
1. `ROS_DISTRO` + `AMENT_PREFIX_PATH` + `CMAKE_PREFIX_PATH` set in the
   build shell (the standard `source /opt/ros/<distro>/setup.bash`).
   `build.rs` discovers the lanelet2 headers and libs from these env
   vars; without them it `panic!`s with a precise error.
2. `lanelet2_core` available — `apt install ros-${ROS_DISTRO}-lanelet2`
   on Ubuntu, or equivalent.
3. `boost-serialization` available — `apt install libboost-serialization-dev`
   on Ubuntu. The same boost version that the integrator's Autoware /
   map server used to produce `LaneletMapBin` must be used here to
   consume it (boost::archive::binary_iarchive is not portable across
   boost versions — see spike report §6.4).
4. Eigen3 available — `apt install libeigen3-dev` (lanelet2_core headers
   include `Eigen/Geometry`). NOTE: `build.rs` does not yet auto-discover
   Eigen's include dir; on distros that install it under
   `/usr/include/eigen3` you currently need `CXXFLAGS=-I/usr/include/eigen3`
   (tracked as a follow-up — see the PR notes). This, and the lanelet2
   serialization-API version mismatch on Jazzy, are concerns of the
   `lanelet2` feature only and do not affect the `ros2` perception build.

Supported ROS distros via r2r 0.9.5: Humble, Iron, Jazzy. Pin the
integrator's Autoware release in the integrator's `package.xml`.

### Phase 2B test fixtures

Tests in `src/corridor/lanelet2.rs::lanelet2_tests` need a fixture
`tests/fixtures/straight_corridor.osm.bin`. The fixture is intentionally
not committed (Boost-version pinning makes a committed fixture brittle).
See `tests/fixtures/README.md` for the one-shot regeneration recipe.

## Why r2r (not rclrs)?

Decision recorded in the S131 discovery report:
1. `cargo build` only — no colcon hookup, matches Kirra's existing build.
2. Async-from-the-ground-up (futures + streams) — matches the
   slow-loop / fast-loop tokio model the design assumes.
3. Runtime-agnostic — composes cleanly with the existing tokio
   ecosystem used elsewhere in the workspace.

The decision is reversible: this adapter is the only crate that touches
ROS, so a swap to rclrs would be bounded.
