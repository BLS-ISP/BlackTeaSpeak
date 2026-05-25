use std::time::Instant;

pub struct DspState<'a> {
    pub hpf_x1: f32,
    pub hpf_y1: f32,
    pub hpf_alpha: f32,
    pub smoothed_ducking: f32,
    pub smoothed_vad: f32,
    pub current_agc_gain: f32,
    pub target_rms: f32,
    pub input_buffer: Vec<f32>,
    pub denoise_buffer: Vec<f32>,
    pub denoise_state: Box<nnnoiseless::DenoiseState<'a>>,
    pub last_voice_activity_time: Instant,
    pub last_emit: Instant,
    pub frame_size: usize,
}

impl<'a> DspState<'a> {
    pub fn new() -> Self {
        Self {
            hpf_x1: 0.0,
            hpf_y1: 0.0,
            hpf_alpha: 0.9896,
            smoothed_ducking: 1.0,
            smoothed_vad: 0.0,
            current_agc_gain: 1.0,
            target_rms: 0.15,
            input_buffer: Vec::new(),
            denoise_buffer: Vec::new(),
            denoise_state: nnnoiseless::DenoiseState::new(),
            last_voice_activity_time: Instant::now(),
            last_emit: Instant::now(),
            frame_size: 960,
        }
    }
}
