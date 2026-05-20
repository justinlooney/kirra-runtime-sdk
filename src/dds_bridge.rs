// src/dds_bridge.rs

pub enum DdsReliability { Reliable, BestEffort }
pub enum DdsDurability { Volatile, TransientLocal }

pub struct DdsQosProfile {
    pub reliability: DdsReliability,
    pub durability: DdsDurability,
    pub deadline_ms: u32,
}

impl DdsQosProfile {
    pub fn critical_actuator_profile() -> Self {
        Self {
            reliability: DdsReliability::Reliable,
            durability: DdsDurability::Volatile,
            deadline_ms: 20,
        }
    }
}

pub struct DdsPublisherBridge;

impl DdsPublisherBridge {
    pub fn wrap_cdr_encapsulation(payload: &[u8]) -> Vec<u8> {
        let mut wrapped = Vec::with_capacity(4 + payload.len());
        wrapped.extend_from_slice(&[0x00, 0x01, 0x00, 0x00]);
        wrapped.extend_from_slice(payload);
        wrapped
    }
}
