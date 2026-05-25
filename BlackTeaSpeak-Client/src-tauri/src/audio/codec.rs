use audiopus::{coder::Encoder, coder::Decoder, Application, Channels, SampleRate};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use tokio::sync::mpsc::{UnboundedSender, UnboundedReceiver};

pub fn spawn_decoder_thread(
    mut rx_opus_in: UnboundedReceiver<(u16, Vec<u8>)>,
    prod: UnboundedSender<f32>,
    client_volumes: Arc<Mutex<HashMap<u16, f32>>>,
) -> Result<(), String> {
    let mut decoder = Decoder::new(SampleRate::Hz48000, Channels::Mono)
        .map_err(|e| format!("Failed to create Opus decoder: {:?}", e))?;

    std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async move {
            while let Some((sender_client_id, opus_packet)) = rx_opus_in.recv().await {
                let mut decoded = vec![0f32; 5760];
                if let Ok(len) = decoder.decode_float(Some(&opus_packet), &mut decoded, false) {
                    decoded.truncate(len);
                    
                    let volume = client_volumes.lock().unwrap().get(&sender_client_id).copied().unwrap_or(1.0);
                    
                    for sample in decoded {
                        let _ = prod.send(sample * volume);
                    }
                }
            }
        });
    });
    Ok(())
}

pub fn spawn_encoder_thread(
    mut rx_raw_audio: UnboundedReceiver<(bool, Vec<f32>)>,
    tx_opus_out: UnboundedSender<(bool, Vec<u8>)>,
) -> Result<(), String> {
    std::thread::spawn(move || {
        if let Ok(mut encoder) = Encoder::new(SampleRate::Hz48000, Channels::Mono, Application::Voip) {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            runtime.block_on(async move {
                while let Some((is_whisper, frame)) = rx_raw_audio.recv().await {
                    let mut out_payload = vec![0u8; 4000];
                    if let Ok(len) = encoder.encode_float(&frame, &mut out_payload) {
                        out_payload.truncate(len);
                        let _ = tx_opus_out.send((is_whisper, out_payload));
                    }
                }
            });
        }
    });
    Ok(())
}
