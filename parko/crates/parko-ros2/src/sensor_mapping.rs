// parko/crates/parko-ros2/src/sensor_mapping.rs
//
// Mapping: incoming ROS 2 sensor message → parko-core `SensorFrame`.
//
// This is one of the two seams the integrator overrides per platform.
// The mapping is intentionally pure (no ROS imports here — the node
// crate provides the ROS-side deserialization and hands the typed
// payload to a `SensorInputMapping`). That keeps this module
// unit-testable on stable and lets the same mapping be used in a
// CARLA harness or a bag-replay test.

use std::collections::HashMap;

use parko_core::backend::{TensorBatch, TensorStorage};
use parko_core::sensor::SensorFrame;

/// Integrator-supplied sensor → tensor mapping. The integrator
/// implements this trait against their concrete sensor message type
/// (a flattened image vector, a lidar batch, fused features, etc.)
/// and hands it to the node.
///
/// Implementations must be `Send + Sync` so the node's drain task can
/// hold an `Arc<dyn SensorInputMapping>`.
pub trait SensorInputMapping: Send + Sync {
    /// The integrator's concrete sensor message type. Project-local —
    /// `r2r::UntypedMessage` deserialised JSON, a hand-rolled struct,
    /// or whatever the sensor publisher emits. The node side
    /// instantiates `Self::Sample` from the r2r untyped subscription
    /// before calling `to_frame`.
    type Sample;

    /// Map one observation to a `SensorFrame`. `frame_id` is a
    /// monotonic counter the caller maintains. `timestamp_ms` is
    /// the wall-clock timestamp of the observation (typically from
    /// `header.stamp` on the ROS side); the staleness check in the
    /// tick pipeline compares this to wall clock at tick time.
    fn to_frame(
        &self,
        frame_id: u64,
        timestamp_ms: u64,
        sample: &Self::Sample,
    ) -> SensorFrame;
}

/// A test-only mapping that wraps a vector of f32 features under a
/// single tensor name. Used by the stable-lane tests in
/// `tick_pipeline_tests` and reusable by integrators as a starting
/// point for a real sensor.
#[derive(Debug, Clone)]
pub struct VectorMapping {
    tensor_name: String,
}

impl VectorMapping {
    #[must_use]
    pub fn new(tensor_name: impl Into<String>) -> Self {
        Self { tensor_name: tensor_name.into() }
    }
}

impl SensorInputMapping for VectorMapping {
    type Sample = Vec<f32>;

    fn to_frame(&self, frame_id: u64, timestamp_ms: u64, sample: &Vec<f32>) -> SensorFrame {
        let mut named_tensors: HashMap<String, TensorStorage<'static>> =
            HashMap::with_capacity(1);
        named_tensors.insert(
            self.tensor_name.clone(),
            TensorStorage::Owned(sample.clone()),
        );
        // `SensorFrame::new` stamps `current_time_ms()` itself; for
        // staleness-correctness we want the timestamp the sensor
        // emitted. Construct the struct directly using its public
        // fields.
        SensorFrame {
            frame_id,
            timestamp_ms,
            payload: TensorBatch {
                named_tensors,
                metadata: HashMap::new(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_mapping_preserves_payload() {
        let m = VectorMapping::new("obs");
        let frame = m.to_frame(42, 1_000, &vec![1.0, 2.0, 3.0]);
        assert_eq!(frame.frame_id, 42);
        assert_eq!(frame.timestamp_ms, 1_000);
        let tensor = frame.payload.named_tensors.get("obs").expect("tensor present");
        assert_eq!(tensor.as_slice(), &[1.0, 2.0, 3.0]);
    }

    #[test]
    fn vector_mapping_is_send_sync() {
        // Compile-time check: the trait object must be `Send + Sync`
        // so the node can pass it across the drain-task boundary.
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<VectorMapping>();
        let _: Box<dyn SensorInputMapping<Sample = Vec<f32>> + Send + Sync>
            = Box::new(VectorMapping::new("obs"));
    }
}
