use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use tauri::Emitter;
use cpal::{StreamConfig, SampleFormat};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender, UnboundedReceiver};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;
use std::collections::HashMap;

pub struct AudioManager {
    input_stream: Option<cpal::Stream>,
    output_stream: Option<cpal::Stream>,
    pub is_mic_muted: Arc<AtomicBool>,
    pub is_speaker_muted: Arc<AtomicBool>,
    pub is_ptt_pressed: Arc<AtomicBool>,
    pub is_whisper_active: Arc<AtomicBool>,
    pub tx_opus_in: UnboundedSender<(u16, Vec<u8>)>,
    pub tx_opus_out: UnboundedSender<(bool, Vec<u8>)>,
    
    pub input_amp: Arc<AtomicU32>,
    pub output_amp: Arc<AtomicU32>,
    pub vad_threshold: Arc<AtomicU32>,
    pub transmission_mode: Arc<Mutex<String>>,
    pub speaker_output_rms: Arc<AtomicU32>,
    pub noise_suppression: Arc<AtomicBool>,
    pub auto_gain_control: Arc<AtomicBool>,
    pub echo_cancellation: Arc<AtomicBool>,
    pub client_volumes: Arc<Mutex<HashMap<u16, f32>>>,
    // Shared receiver for playback
    cons_rb: Arc<Mutex<tokio::sync::mpsc::UnboundedReceiver<f32>>>,
}

pub fn f32_to_bits(v: f32) -> u32 {
    v.to_bits()
}

fn f32_from_bits(v: u32) -> f32 {
    f32::from_bits(v)
}

fn get_device_name(dev: &cpal::Device) -> String {
    use cpal::traits::DeviceTrait;
    let name = dev.name().unwrap_or_else(|_| "Unknown Device".to_string());
    if let Ok(desc) = dev.description() {
        let extended = desc.extended();
        if !extended.is_empty() {
            return extended[0].clone();
        }
        if let Some(driver) = desc.driver() {
            return format!("{} ({})", name, driver);
        }
    }
    name
}

impl AudioManager {
    pub fn new(app_handle: Option<tauri::AppHandle>) -> Result<(Self, UnboundedReceiver<(bool, Vec<u8>)>), String> {
        let is_mic_muted = Arc::new(AtomicBool::new(false));
        let is_speaker_muted = Arc::new(AtomicBool::new(false));
        let is_ptt_pressed = Arc::new(AtomicBool::new(false));
        let is_whisper_active = Arc::new(AtomicBool::new(false));
        
        let input_amp = Arc::new(AtomicU32::new(f32_to_bits(1.0)));
        let output_amp = Arc::new(AtomicU32::new(f32_to_bits(1.0)));
        let vad_threshold = Arc::new(AtomicU32::new(f32_to_bits(0.05)));
        let transmission_mode = Arc::new(Mutex::new(String::from("voice_activation")));
        let speaker_output_rms = Arc::new(AtomicU32::new(f32_to_bits(0.0)));
        let noise_suppression = Arc::new(AtomicBool::new(true));
        let auto_gain_control = Arc::new(AtomicBool::new(true));
        let echo_cancellation = Arc::new(AtomicBool::new(true));
        let client_volumes = Arc::new(Mutex::new(HashMap::new()));

        let (tx_opus_out_sender, tx_opus_out_receiver) = unbounded_channel::<(bool, Vec<u8>)>();
        let (tx_opus_in_sender, mut rx_opus_in_receiver) = unbounded_channel::<(u16, Vec<u8>)>();

        let (prod, cons) = unbounded_channel::<f32>();
        let cons_rb = Arc::new(Mutex::new(cons));

        super::codec::spawn_decoder_thread(rx_opus_in_receiver, prod, client_volumes.clone())?;

        let mut manager = Self {
            input_stream: None,
            output_stream: None,
            is_mic_muted,
            is_speaker_muted,
            is_ptt_pressed,
            is_whisper_active,
            tx_opus_in: tx_opus_in_sender,
            tx_opus_out: tx_opus_out_sender,
            input_amp,
            output_amp,
            vad_threshold,
            transmission_mode,
            speaker_output_rms,
            noise_suppression,
            auto_gain_control,
            echo_cancellation,
            client_volumes,
            cons_rb,
        };

        // Try to initialize default devices
        let _ = manager.set_input_device(None, app_handle.clone());
        let _ = manager.set_output_device(None, app_handle);

        Ok((manager, tx_opus_out_receiver))
    }
    
    pub fn list_devices() -> Result<(Vec<String>, Vec<String>), String> {
        let host = cpal::default_host();
        let mut inputs = Vec::new();
        let mut outputs = Vec::new();
        
        let mut name_counts = std::collections::HashMap::new();
        if let Ok(devices) = host.input_devices() {
            for dev in devices {
                let base_name = get_device_name(&dev);
                let count = name_counts.entry(base_name.clone()).or_insert(0);
                *count += 1;
                let final_name = if *count > 1 {
                    format!("{} ({})", base_name, count)
                } else {
                    base_name
                };
                inputs.push(final_name);
            }
        }
        
        let mut name_counts = std::collections::HashMap::new();
        if let Ok(devices) = host.output_devices() {
            for dev in devices {
                let base_name = get_device_name(&dev);
                let count = name_counts.entry(base_name.clone()).or_insert(0);
                *count += 1;
                let final_name = if *count > 1 {
                    format!("{} ({})", base_name, count)
                } else {
                    base_name
                };
                outputs.push(final_name);
            }
        }
        
        Ok((inputs, outputs))
    }

    pub fn set_input_device(&mut self, device_name: Option<String>, app_handle: Option<tauri::AppHandle>) -> Result<(), String> {
        let host = cpal::default_host();
        let device = if let Some(name) = device_name {
            let mut found_device = None;
            let mut name_counts = std::collections::HashMap::new();
            for dev in host.input_devices().map_err(|e| e.to_string())? {
                let base_name = get_device_name(&dev);
                let count = name_counts.entry(base_name.clone()).or_insert(0);
                *count += 1;
                let final_name = if *count > 1 {
                    format!("{} ({})", base_name, count)
                } else {
                    base_name
                };
                if final_name == name {
                    found_device = Some(dev);
                    break;
                }
            }
            found_device.ok_or_else(|| "Input device not found".to_string())?
        } else {
            host.default_input_device().ok_or("No default input device found")?
        };

        let config = StreamConfig {
            channels: 1,
            sample_rate: 48000,
            buffer_size: cpal::BufferSize::Default,
        };

        let mic_muted = self.is_mic_muted.clone();
        let ptt_pressed = self.is_ptt_pressed.clone();
        let whisper_active = self.is_whisper_active.clone();
        let input_amp = self.input_amp.clone();
        let vad_threshold = self.vad_threshold.clone();
        let transmission_mode = self.transmission_mode.clone();
        let speaker_output_rms = self.speaker_output_rms.clone();
        let noise_suppression = self.noise_suppression.clone();
        let auto_gain_control = self.auto_gain_control.clone();
        let echo_cancellation = self.echo_cancellation.clone();
        let tx_opus = self.tx_opus_out.clone();
        let last_talking_status = std::sync::Arc::new(std::sync::Mutex::new(false));

        let (tx_raw_audio, mut rx_raw_audio) = unbounded_channel::<(bool, Vec<f32>)>();
        let tx_opus_for_thread = self.tx_opus_out.clone();

        super::codec::spawn_encoder_thread(rx_raw_audio, tx_opus_for_thread)?;
        
        let mut dsp = super::dsp::DspState::new();

        let stream = device.build_input_stream(
            &config,
            move |data: &[f32], _: &_| {
                if mic_muted.load(Ordering::Relaxed) {
                    return;
                }
                
                let mode = transmission_mode.lock().unwrap().clone();
                let is_ptt = mode == "push_to_talk";
                let is_vad = mode == "voice_activation";
                let is_whisper = whisper_active.load(Ordering::Relaxed);
                
                if !is_whisper && is_ptt && !ptt_pressed.load(Ordering::Relaxed) {
                    return;
                }

                let amp = f32_from_bits(input_amp.load(Ordering::Relaxed));
                let thresh = f32_from_bits(vad_threshold.load(Ordering::Relaxed));
                let speaker_rms = f32_from_bits(speaker_output_rms.load(Ordering::Relaxed));
                let use_ns = noise_suppression.load(Ordering::Relaxed);
                let use_agc = auto_gain_control.load(Ordering::Relaxed);
                let use_aec = echo_cancellation.load(Ordering::Relaxed);
                
                // Echo Suppression (Ducking)
                let target_ducking = if use_aec && speaker_rms > 0.01 { 0.05 } else { 1.0 };
                
                // Attack (fast ducking), Release (slow recovery)
                if target_ducking < dsp.smoothed_ducking {
                    dsp.smoothed_ducking = dsp.smoothed_ducking * 0.5 + target_ducking * 0.5; // Fast attack
                } else {
                    dsp.smoothed_ducking = dsp.smoothed_ducking * 0.995 + target_ducking * 0.005; // Slow release (~150ms)
                }
                
                // Process High-Pass Filter
                let mut hpf_data = Vec::with_capacity(data.len());
                for &s in data.iter() {
                    let y = dsp.hpf_alpha * (dsp.hpf_y1 + s - dsp.hpf_x1);
                    dsp.hpf_x1 = s;
                    dsp.hpf_y1 = y;
                    hpf_data.push(y);
                }
                
                // Calculate Raw Input RMS for AGC
                let mut raw_sum_sq = 0.0;
                for &s in hpf_data.iter() { raw_sum_sq += s * s; }
                let raw_rms = (raw_sum_sq / hpf_data.len() as f32).sqrt();
                
                // AGC (Automatic Gain Control) with Noise Gating
                // Only adjust gain if someone is actually talking (using smoothed_vad as gate)
                if use_agc && dsp.smoothed_ducking > 0.5 && raw_rms > 0.001 && dsp.smoothed_vad > 0.4 {
                    let agc_target_gain = (dsp.target_rms / raw_rms).clamp(0.1, 5.0);
                    
                    if dsp.current_agc_gain < agc_target_gain {
                        // Slow attack (don't ramp up too fast during speech)
                        dsp.current_agc_gain = dsp.current_agc_gain * 0.995 + agc_target_gain * 0.005; 
                    } else {
                        // Fast release (if they shout, pull it down quickly)
                        dsp.current_agc_gain = dsp.current_agc_gain * 0.90 + agc_target_gain * 0.10;
                    }
                } else if !use_agc {
                    dsp.current_agc_gain = 1.0;
                }
                
                let final_amp = amp * dsp.smoothed_ducking * dsp.current_agc_gain;
                
                // Add all new samples to denoise_buffer with amplification
                dsp.denoise_buffer.extend(hpf_data.iter().map(|&s| s * final_amp));
                
                let mut vad_prob_sum = 0.0;
                let mut vad_prob_count = 0;
                let mut max_rms = 0.0;

                while dsp.denoise_buffer.len() >= 480 {
                    let mut frame_out = [0.0f32; 480];
                    let vad_prob = if use_ns {
                        dsp.denoise_state.process_frame(&mut frame_out, &dsp.denoise_buffer[0..480])
                    } else {
                        frame_out.copy_from_slice(&dsp.denoise_buffer[0..480]);
                        // If NS is off, we still need a VAD probability. We can estimate it simply based on RMS.
                        let mut sum_sq = 0.0;
                        for &sample in frame_out.iter() { sum_sq += sample * sample; }
                        let rms = (sum_sq / 480.0).sqrt();
                        if rms > 0.005 { 1.0 } else { 0.0 }
                    };
                    
                    let mut sum_sq = 0.0;
                    for &sample in frame_out.iter() {
                        sum_sq += sample * sample;
                    }
                    let rms = (sum_sq / 480.0).sqrt();
                    if rms > max_rms { max_rms = rms; }

                    vad_prob_sum += vad_prob;
                    vad_prob_count += 1;
                    
                    dsp.input_buffer.extend_from_slice(&frame_out);
                    dsp.denoise_buffer.drain(0..480);
                }

                if let Some(app) = &app_handle {
                    if dsp.last_emit.elapsed().as_millis() > 50 {
                        let _ = app.emit("audio_levels_input", max_rms);
                        dsp.last_emit = std::time::Instant::now();
                    }
                }

                let mut is_talking_now = true;
                if !is_whisper && is_vad {
                    let avg_vad = if vad_prob_count > 0 { vad_prob_sum / vad_prob_count as f32 } else { 0.0 };
                    
                    // Smooth the VAD probability
                    dsp.smoothed_vad = dsp.smoothed_vad * 0.8 + avg_vad * 0.2;
                    
                    // The vad_threshold slider is 0.0-1.0
                    if dsp.smoothed_vad >= thresh {
                        dsp.last_voice_activity_time = std::time::Instant::now();
                    }
                    
                    if dsp.last_voice_activity_time.elapsed().as_millis() > 1000 {
                        is_talking_now = false;
                    }
                } else if is_ptt {
                    // Force gate open during PTT or Whisper
                    dsp.smoothed_vad = 1.0;
                }
                
                if let Some(app) = &app_handle {
                    if let Ok(mut last_status) = last_talking_status.lock() {
                        if *last_status != is_talking_now {
                            let _ = app.emit("is_transmitting", is_talking_now);
                            *last_status = is_talking_now;
                        }
                    }
                }

                if !is_talking_now {
                    dsp.input_buffer.clear();
                    return; // Below threshold and hold time expired, do not transmit
                }

                while dsp.input_buffer.len() >= dsp.frame_size {
                    let frame: Vec<f32> = dsp.input_buffer.drain(0..dsp.frame_size).collect();
                    let _ = tx_raw_audio.send((is_whisper, frame));
                }
            },
            |err| eprintln!("Input stream error: {}", err),
            None,
        ).map_err(|e| format!("Failed to build input stream: {:?}", e))?;

        stream.play().map_err(|e| format!("Failed to play input stream: {:?}", e))?;
        self.input_stream = Some(stream);
        Ok(())
    }

    pub fn set_output_device(&mut self, device_name: Option<String>, app_handle: Option<tauri::AppHandle>) -> Result<(), String> {
        let host = cpal::default_host();
        let device = if let Some(name) = device_name {
            let mut found_device = None;
            let mut name_counts = std::collections::HashMap::new();
            for dev in host.output_devices().map_err(|e| e.to_string())? {
                let base_name = get_device_name(&dev);
                let count = name_counts.entry(base_name.clone()).or_insert(0);
                *count += 1;
                let final_name = if *count > 1 {
                    format!("{} ({})", base_name, count)
                } else {
                    base_name
                };
                if final_name == name {
                    found_device = Some(dev);
                    break;
                }
            }
            found_device.ok_or_else(|| "Output device not found".to_string())?
        } else {
            host.default_output_device().ok_or("No default output device found")?
        };

        let config = StreamConfig {
            channels: 1,
            sample_rate: 48000,
            buffer_size: cpal::BufferSize::Default,
        };

        let speaker_muted = self.is_speaker_muted.clone();
        let output_amp = self.output_amp.clone();
        let cons_rb = self.cons_rb.clone();
        let speaker_output_rms = self.speaker_output_rms.clone();

        let mut last_emit = std::time::Instant::now();

        let stream = device.build_output_stream(
            &config,
            move |data: &mut [f32], _: &_| {
                let mut cons = cons_rb.lock().unwrap();
                let amp = f32_from_bits(output_amp.load(Ordering::Relaxed));
                
                if speaker_muted.load(Ordering::Relaxed) {
                    for sample in data.iter_mut() { *sample = 0.0; }
                    speaker_output_rms.store(f32_to_bits(0.0), Ordering::Relaxed);
                    return;
                }
                
                let mut sum_sq = 0.0;
                for sample in data.iter_mut() {
                    let s = cons.try_recv().unwrap_or(0.0) * amp;
                    *sample = s;
                    sum_sq += s * s;
                }
                
                let current_rms = (sum_sq / data.len() as f32).sqrt();
                speaker_output_rms.store(f32_to_bits(current_rms), Ordering::Relaxed);

                // Output RMS to frontend
                if let Some(app) = &app_handle {
                    if last_emit.elapsed().as_millis() > 50 {
                        let _ = app.emit("audio_levels_output", current_rms);
                        last_emit = std::time::Instant::now();
                    }
                }
            },
            |err| eprintln!("Output stream error: {}", err),
            None,
        ).map_err(|e| format!("Failed to build output stream: {:?}", e))?;

        stream.play().map_err(|e| format!("Failed to play output stream: {:?}", e))?;
        self.output_stream = Some(stream);
        Ok(())
    }
}
