use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use tauri::Emitter;
use cpal::{StreamConfig, SampleFormat};
use audiopus::{coder::Encoder, coder::Decoder, Application, Channels, SampleRate};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender, UnboundedReceiver};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

pub struct AudioManager {
    input_stream: Option<cpal::Stream>,
    output_stream: Option<cpal::Stream>,
    pub is_mic_muted: Arc<AtomicBool>,
    pub is_speaker_muted: Arc<AtomicBool>,
    pub is_ptt_pressed: Arc<AtomicBool>,
    pub is_whisper_active: Arc<AtomicBool>,
    pub tx_opus_in: UnboundedSender<Vec<u8>>,
    pub tx_opus_out: UnboundedSender<(bool, Vec<u8>)>,
    
    pub input_amp: Arc<AtomicU32>,
    pub output_amp: Arc<AtomicU32>,
    pub vad_threshold: Arc<AtomicU32>,
    pub transmission_mode: Arc<Mutex<String>>,
    // Shared receiver for playback
    cons_rb: Arc<Mutex<tokio::sync::mpsc::UnboundedReceiver<f32>>>,
}

pub(crate) fn f32_to_bits(v: f32) -> u32 {
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

        let (tx_opus_out_sender, tx_opus_out_receiver) = unbounded_channel::<(bool, Vec<u8>)>();
        let (tx_opus_in_sender, mut rx_opus_in_receiver) = unbounded_channel::<Vec<u8>>();

        let (prod, cons) = unbounded_channel::<f32>();
        let cons_rb = Arc::new(Mutex::new(cons));

        let mut decoder = Decoder::new(SampleRate::Hz48000, Channels::Mono)
            .map_err(|e| format!("Failed to create Opus decoder: {:?}", e))?;

        std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async move {
                while let Some(opus_packet) = rx_opus_in_receiver.recv().await {
                    let mut decoded = vec![0f32; 5760];
                    if let Ok(len) = decoder.decode_float(Some(&opus_packet), &mut decoded, false) {
                        decoded.truncate(len);
                        for sample in decoded {
                            let _ = prod.send(sample);
                        }
                    }
                }
            });
        });

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
        let tx_opus = self.tx_opus_out.clone();
        let last_talking_status = std::sync::Arc::new(std::sync::Mutex::new(false));

        let (tx_raw_audio, mut rx_raw_audio) = unbounded_channel::<(bool, Vec<f32>)>();
        let tx_opus_for_thread = self.tx_opus_out.clone();

        std::thread::spawn(move || {
            if let Ok(mut encoder) = Encoder::new(SampleRate::Hz48000, Channels::Mono, Application::Voip) {
                let runtime = tokio::runtime::Runtime::new().unwrap();
                runtime.block_on(async move {
                    while let Some((is_whisper, frame)) = rx_raw_audio.recv().await {
                        let mut out_payload = vec![0u8; 4000];
                        if let Ok(len) = encoder.encode_float(&frame, &mut out_payload) {
                            out_payload.truncate(len);
                            let _ = tx_opus_for_thread.send((is_whisper, out_payload));
                        }
                    }
                });
            }
        });
        
        let frame_size = 960;
        let mut input_buffer: Vec<f32> = Vec::new();
        let mut last_emit = std::time::Instant::now();

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
                
                // Calculate RMS for VAD
                let mut sum_sq = 0.0;
                for &sample in data {
                    let amp_sample = sample * amp;
                    sum_sq += amp_sample * amp_sample;
                }
                let rms = (sum_sq / data.len() as f32).sqrt();
                
                if let Some(app) = &app_handle {
                    if last_emit.elapsed().as_millis() > 50 {
                        let _ = app.emit("audio_levels_input", rms);
                        last_emit = std::time::Instant::now();
                    }
                }

                let mut is_talking_now = true;
                if !is_whisper && is_vad && rms < thresh {
                    is_talking_now = false;
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
                    return; // Below threshold, do not transmit
                }

                input_buffer.extend(data.iter().map(|&s| s * amp));
                
                while input_buffer.len() >= frame_size {
                    let frame: Vec<f32> = input_buffer.drain(0..frame_size).collect();
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

        let mut last_emit = std::time::Instant::now();

        let stream = device.build_output_stream(
            &config,
            move |data: &mut [f32], _: &_| {
                let mut cons = cons_rb.lock().unwrap();
                let amp = f32_from_bits(output_amp.load(Ordering::Relaxed));
                
                if speaker_muted.load(Ordering::Relaxed) {
                    for sample in data.iter_mut() { *sample = 0.0; }
                    return;
                }
                
                for sample in data.iter_mut() {
                    *sample = cons.try_recv().unwrap_or(0.0) * amp;
                }

                // Output RMS
                if let Some(app) = &app_handle {
                    if last_emit.elapsed().as_millis() > 50 {
                        let mut sum_sq = 0.0;
                        for &sample in data.iter() {
                            sum_sq += sample * sample;
                        }
                        let rms = (sum_sq / data.len() as f32).sqrt();
                        let _ = app.emit("audio_levels_output", rms);
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
