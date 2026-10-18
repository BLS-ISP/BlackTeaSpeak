use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0, z: 0.0 };
    pub const FORWARD: Self = Self { x: 0.0, y: 0.0, z: 1.0 };
    pub const UP: Self = Self { x: 0.0, y: 1.0, z: 0.0 };

    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn sub(self, other: Self) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
            z: self.z - other.z,
        }
    }

    pub fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    pub fn cross(self, other: Self) -> Self {
        Self {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }

    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }

    pub fn normalized(self) -> Self {
        let len = self.length();
        if len > 1e-5 {
            Self {
                x: self.x / len,
                y: self.y / len,
                z: self.z / len,
            }
        } else {
            Self::ZERO
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListenerPose {
    pub position: Vec3,
    pub forward: Vec3,
    pub up: Vec3,
}

impl Default for ListenerPose {
    fn default() -> Self {
        Self {
            position: Vec3::ZERO,
            forward: Vec3::FORWARD,
            up: Vec3::UP,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientSpatialPosition {
    pub client_id: u16,
    pub position: Vec3,
    pub channel_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpatialAudioStateSnapshot {
    pub enabled: bool,
    pub min_distance: f32,
    pub max_distance: f32,
    pub rolloff_factor: f32,
    pub listener: ListenerPose,
    pub clients: Vec<ClientSpatialPosition>,
}

#[derive(Debug, Clone)]
pub struct SpatialAudioEngine {
    pub enabled: bool,
    pub min_distance: f32,
    pub max_distance: f32,
    pub rolloff_factor: f32,
    pub listener: ListenerPose,
    pub client_positions: HashMap<u16, ClientSpatialPosition>,
}

impl Default for SpatialAudioEngine {
    fn default() -> Self {
        Self {
            enabled: true,
            min_distance: 1.5,
            max_distance: 45.0,
            rolloff_factor: 1.35,
            listener: ListenerPose::default(),
            client_positions: HashMap::new(),
        }
    }
}

impl SpatialAudioEngine {
    pub fn set_client_position(&mut self, client_id: u16, position: Vec3, channel_name: Option<String>) {
        self.client_positions.insert(
            client_id,
            ClientSpatialPosition {
                client_id,
                position,
                channel_name,
            },
        );
    }

    pub fn remove_client_position(&mut self, client_id: u16) {
        self.client_positions.remove(&client_id);
    }

    pub fn set_listener_pose(&mut self, position: Vec3, forward: Option<Vec3>, up: Option<Vec3>) {
        self.listener.position = position;
        if let Some(f) = forward {
            let norm = f.normalized();
            if norm.length() > 0.1 {
                self.listener.forward = norm;
            }
        }
        if let Some(u) = up {
            let norm = u.normalized();
            if norm.length() > 0.1 {
                self.listener.up = norm;
            }
        }
    }

    pub fn reset(&mut self) {
        self.listener = ListenerPose::default();
        self.client_positions.clear();
    }

    pub fn snapshot(&self) -> SpatialAudioStateSnapshot {
        let mut clients: Vec<ClientSpatialPosition> = self.client_positions.values().cloned().collect();
        clients.sort_by_key(|c| c.client_id);
        SpatialAudioStateSnapshot {
            enabled: self.enabled,
            min_distance: self.min_distance,
            max_distance: self.max_distance,
            rolloff_factor: self.rolloff_factor,
            listener: self.listener.clone(),
            clients,
        }
    }

    /// Computes `(left_gain, right_gain)` for a speaking `client_id`.
    /// When spatial audio is disabled or the client has no 3D coordinates assigned,
    /// returns `(1.0, 1.0)` (centered unity gain).
    pub fn compute_stereo_gains(&self, client_id: u16) -> (f32, f32) {
        if !self.enabled {
            return (1.0, 1.0);
        }
        let Some(client_pos) = self.client_positions.get(&client_id) else {
            return (1.0, 1.0);
        };

        let delta = client_pos.position.sub(self.listener.position);
        let distance = delta.length();
        if distance < 1e-4 {
            return (1.0, 1.0);
        }

        // Distance attenuation
        let min_d = self.min_distance.max(0.1);
        let max_d = self.max_distance.max(min_d + 0.5);
        let attenuation = if distance <= min_d {
            1.0
        } else if distance >= max_d {
            0.0
        } else {
            let t = ((distance - min_d) / (max_d - min_d)).clamp(0.0, 1.0);
            (1.0 - t).powf(self.rolloff_factor.max(0.2))
        };

        if attenuation <= 0.0001 {
            return (0.0, 0.0);
        }

        // Compute listener right basis vector: up x forward -> +X when forward=+Z, up=+Y
        let forward = self.listener.forward.normalized();
        let up = self.listener.up.normalized();
        let mut right = up.cross(forward).normalized();
        if right.length() < 1e-4 {
            right = Vec3::new(1.0, 0.0, 0.0);
        }

        let dir = delta.normalized();
        let pan = dir.dot(right).clamp(-1.0, 1.0);

        // Equal-power panning mapped from [-1.0, 1.0] -> [0, PI/2], scaled by SQRT_2 so center pan=0 is (1.0, 1.0)
        let angle = (pan + 1.0) * 0.25 * std::f32::consts::PI;
        let left_gain = (attenuation * angle.cos() * std::f32::consts::SQRT_2).clamp(0.0, 1.5);
        let right_gain = (attenuation * angle.sin() * std::f32::consts::SQRT_2).clamp(0.0, 1.5);

        (left_gain, right_gain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_stereo_panning_and_distance_attenuation() {
        let mut engine = SpatialAudioEngine::default();
        // Default unpositioned client -> unity stereo
        assert_eq!(engine.compute_stereo_gains(1), (1.0, 1.0));

        // Client directly to the right (+X = 5m) -> louder in right ear than left ear
        engine.set_client_position(2, Vec3::new(5.0, 0.0, 0.0), None);
        let (l_right, r_right) = engine.compute_stereo_gains(2);
        assert!(r_right > 0.7, "expected strong right gain, got {r_right}");
        assert!(l_right < 0.05, "expected near-zero left gain, got {l_right}");

        // Client directly to the left (-X = -5m) -> louder in left ear than right ear
        engine.set_client_position(3, Vec3::new(-5.0, 0.0, 0.0), None);
        let (l_left, r_left) = engine.compute_stereo_gains(3);
        assert!(l_left > 0.7, "expected strong left gain, got {l_left}");
        assert!(r_left < 0.05, "expected near-zero right gain, got {r_left}");

        // Client beyond max_distance (100m away) -> silent (0.0, 0.0)
        engine.set_client_position(4, Vec3::new(0.0, 0.0, 100.0), None);
        assert_eq!(engine.compute_stereo_gains(4), (0.0, 0.0));
    }
}
