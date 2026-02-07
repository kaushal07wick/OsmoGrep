use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;
use std::net::{TcpListener, TcpStream};

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use cpal::{SampleFormat, Stream};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::Sender as CbSender;
use serde_json::json;
use tungstenite::{accept, connect, Message};
use url::Url;

use std::sync::mpsc::{Receiver, Sender};

const TARGET_SAMPLE_RATE: u32 = 16_000;
const CHUNK_MS: u32 = 100;
const CHUNK_SAMPLES: usize = (TARGET_SAMPLE_RATE as usize * CHUNK_MS as usize) / 1000;

#[derive(Debug, Clone)]
pub enum VoiceCommand {
    Start { url: String, model: String },
    Stop,
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum VoiceEvent {
    Connected,
    Disconnected,
    Partial(String),
    Final(String),
    Error(String),
    Status(String),
}

pub fn spawn_voice_worker(
    cmd_rx: Receiver<VoiceCommand>,
    evt_tx: Sender<VoiceEvent>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        loop {
            match cmd_rx.recv() {
                Ok(VoiceCommand::Start { url, model }) => {
                    if let Err(e) = run_session(&cmd_rx, &evt_tx, &url, &model) {
                        let _ = evt_tx.send(VoiceEvent::Error(e));
                    }
                }
                Ok(VoiceCommand::Stop) => {
                    let _ = evt_tx.send(VoiceEvent::Status(
                        "Voice not running.".into(),
                    ));
                }
                Ok(VoiceCommand::Shutdown) | Err(_) => {
                    break;
                }
            }
        }
    })
}

pub fn spawn_voice_proxy_worker(
    listen_addr: String,
    vllm_url: String,
    model: String,
    evt_tx: Sender<VoiceEvent>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let listener = match TcpListener::bind(&listen_addr) {
            Ok(listener) => listener,
            Err(e) => {
                let _ = evt_tx.send(VoiceEvent::Error(format!(
                    "Voice proxy bind failed: {e}"
                )));
                return;
            }
        };

        let _ = evt_tx.send(VoiceEvent::Status(format!(
            "voice proxy listening on {listen_addr}"
        )));

        for stream in listener.incoming() {
            let stream = match stream {
                Ok(stream) => stream,
                Err(e) => {
                    let _ = evt_tx.send(VoiceEvent::Error(format!(
                        "Voice proxy accept failed: {e}"
                    )));
                    continue;
                }
            };

            if let Err(e) = handle_proxy_connection(
                stream,
                &vllm_url,
                &model,
                &evt_tx,
            ) {
                let _ = evt_tx.send(VoiceEvent::Error(e));
            }
        }
    })
}

fn run_session(
    cmd_rx: &Receiver<VoiceCommand>,
    evt_tx: &Sender<VoiceEvent>,
    url: &str,
    model: &str,
) -> Result<(), String> {
    let url = normalize_ws_url(url)?;

    let (mut ws, _resp) = connect(url.as_str()).map_err(|e| e.to_string())?;

    if let tungstenite::stream::MaybeTlsStream::Plain(stream) = ws.get_mut() {
        let _ = stream.set_read_timeout(Some(Duration::from_millis(30)));
    }

    // Wait for session.created, ignore payload.
    let _ = ws.read();

    let session_update = json!({
        "type": "session.update",
        "model": model,
    });

    ws.send(Message::Text(session_update.to_string()))
        .map_err(|e| e.to_string())?;

    // vLLM docs suggest committing before streaming.
    let commit = json!({
        "type": "input_audio_buffer.commit",
    });
    ws.send(Message::Text(commit.to_string()))
        .map_err(|e| e.to_string())?;

    let (audio_tx, audio_rx) = crossbeam_channel::bounded::<Vec<i16>>(8);
    let stop = Arc::new(AtomicBool::new(false));

    let stream = start_audio_capture(audio_tx, stop.clone())
        .map_err(|e| format!("Audio capture failed: {e}"))?;
    stream
        .play()
        .map_err(|e| format!("Audio stream failed to start: {e}"))?;

    let _ = evt_tx.send(VoiceEvent::Connected);

    // Main loop: interleave sending audio and reading server events.
    loop {
        // Check for control commands.
        match cmd_rx.try_recv() {
            Ok(VoiceCommand::Stop) => {
                stop.store(true, Ordering::SeqCst);
                let final_commit = json!({
                    "type": "input_audio_buffer.commit",
                    "final": true,
                });
                let _ = ws.send(Message::Text(final_commit.to_string()));
                let _ = ws.close(None);
                let _ = evt_tx.send(VoiceEvent::Disconnected);
                break;
            }
            Ok(VoiceCommand::Shutdown) => {
                stop.store(true, Ordering::SeqCst);
                let _ = ws.close(None);
                break;
            }
            Ok(VoiceCommand::Start { .. }) => {
                let _ = evt_tx.send(VoiceEvent::Status(
                    "Voice already running.".into(),
                ));
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                stop.store(true, Ordering::SeqCst);
                let _ = ws.close(None);
                break;
            }
        }

        // Send audio chunk if available.
        if let Ok(chunk) = audio_rx.recv_timeout(Duration::from_millis(20)) {
            let mut bytes = Vec::with_capacity(chunk.len() * 2);
            for s in chunk {
                bytes.extend_from_slice(&s.to_le_bytes());
            }

            let audio_b64 = B64.encode(&bytes);
            let msg = json!({
                "type": "input_audio_buffer.append",
                "audio": audio_b64,
            });
            let _ = ws.send(Message::Text(msg.to_string()));
        }

        // Read server events with a short timeout.
        match ws.read() {
            Ok(Message::Text(text)) => {
                handle_server_event(evt_tx, &text);
            }
            Ok(Message::Close(_)) => {
                let _ = evt_tx.send(VoiceEvent::Disconnected);
                break;
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                // no-op, keep looping
            }
            Err(e) => {
                let _ = evt_tx.send(VoiceEvent::Error(e.to_string()));
                break;
            }
        }
    }

    Ok(())
}

fn handle_server_event(evt_tx: &Sender<VoiceEvent>, text: &str) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return;
    };

    let Some(typ) = value.get("type").and_then(|v| v.as_str()) else {
        return;
    };

    match typ {
        "transcription.delta" => {
            if let Some(delta) = value.get("delta").and_then(|v| v.as_str()) {
                let _ = evt_tx.send(VoiceEvent::Partial(delta.to_string()));
            }
        }
        "transcription.done" => {
            if let Some(text) = value.get("text").and_then(|v| v.as_str()) {
                let _ = evt_tx.send(VoiceEvent::Final(text.to_string()));
            }
        }
        "error" => {
            let msg = value
                .get("error")
                .or_else(|| value.get("message"))
                .and_then(|v| v.as_str());
            if let Some(msg) = msg {
                let _ = evt_tx.send(VoiceEvent::Error(msg.to_string()));
            }
        }
        _ => {}
    }
}

fn normalize_ws_url(raw: &str) -> Result<Url, String> {
    let candidate = if raw.contains("://") {
        raw.to_string()
    } else {
        format!("ws://{raw}")
    };

    let mut url = Url::parse(&candidate).map_err(|e| e.to_string())?;
    match url.scheme() {
        "ws" | "wss" => Ok(url),
        "http" => {
            url.set_scheme("ws")
                .map_err(|_| "Invalid ws URL".to_string())?;
            Ok(url)
        }
        "https" => {
            url.set_scheme("wss")
                .map_err(|_| "Invalid wss URL".to_string())?;
            Ok(url)
        }
        _ => Err("Unsupported URL scheme for VLLM_REALTIME_URL".into()),
    }
}

fn handle_proxy_connection(
    stream: TcpStream,
    vllm_url: &str,
    model: &str,
    evt_tx: &Sender<VoiceEvent>,
) -> Result<(), String> {
    let mut client_ws = accept(stream).map_err(|e| e.to_string())?;
    let _ = evt_tx.send(VoiceEvent::Status(
        "voice proxy client connected".into(),
    ));
    let url = normalize_ws_url(vllm_url)?;
    let (mut vllm_ws, _resp) = connect(url.as_str()).map_err(|e| e.to_string())?;

    if let tungstenite::stream::MaybeTlsStream::Plain(stream) = vllm_ws.get_mut() {
        let _ = stream.set_read_timeout(Some(Duration::from_millis(30)));
    }
    let _ = client_ws.get_mut().set_read_timeout(Some(Duration::from_millis(30)));

    // Wait for session.created from vLLM.
    let _ = vllm_ws.read();

    let session_update = json!({
        "type": "session.update",
        "model": model,
    });
    vllm_ws
        .send(Message::Text(session_update.to_string()))
        .map_err(|e| e.to_string())?;

    let _ = evt_tx.send(VoiceEvent::Connected);

    let mut seen_audio = false;
    let mut pending_commit = false;

    loop {
        // Read from client and forward to vLLM.
        match client_ws.read() {
            Ok(Message::Text(text)) => {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                    let is_session_update = value
                        .get("type")
                        .and_then(|v| v.as_str())
                        == Some("session.update");
                    let typ = value.get("type").and_then(|v| v.as_str());
                    if is_session_update {
                        // ignore client session.update; proxy controls model
                    } else if typ == Some("input_audio_buffer.commit") && !seen_audio {
                        pending_commit = true;
                    } else {
                        if typ == Some("input_audio_buffer.append") {
                            seen_audio = true;
                        }
                        let _ = vllm_ws.send(Message::Text(text));
                        if pending_commit && seen_audio {
                            pending_commit = false;
                            let commit = json!({
                                "type": "input_audio_buffer.commit",
                            });
                            let _ = vllm_ws.send(Message::Text(commit.to_string()));
                        }
                    }
                } else {
                    let _ = vllm_ws.send(Message::Text(text));
                }
            }
            Ok(Message::Close(_)) => {
                let _ = vllm_ws.close(None);
                let _ = evt_tx.send(VoiceEvent::Status(
                    "voice proxy client disconnected".into(),
                ));
                let _ = evt_tx.send(VoiceEvent::Disconnected);
                break;
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {}
            Err(e) => {
                let _ = vllm_ws.close(None);
                return Err(e.to_string());
            }
        }

        // Read from vLLM and forward to client + UI.
        match vllm_ws.read() {
            Ok(Message::Text(text)) => {
                handle_server_event(evt_tx, &text);
                let _ = client_ws.send(Message::Text(text));
            }
            Ok(Message::Close(_)) => {
                let _ = client_ws.close(None);
                let _ = evt_tx.send(VoiceEvent::Status(
                    "voice proxy server disconnected".into(),
                ));
                let _ = evt_tx.send(VoiceEvent::Disconnected);
                break;
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {}
            Err(e) => {
                let _ = client_ws.close(None);
                return Err(e.to_string());
            }
        }
    }

    Ok(())
}

fn start_audio_capture(
    audio_tx: CbSender<Vec<i16>>,
    stop: Arc<AtomicBool>,
) -> Result<Stream, String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or("No input audio device found")?;

    let default_config = device
        .default_input_config()
        .map_err(|e: cpal::DefaultStreamConfigError| e.to_string())?;

    let sample_format = default_config.sample_format();
    let config = cpal::StreamConfig {
        channels: default_config.channels(),
        sample_rate: default_config.sample_rate(),
        buffer_size: cpal::BufferSize::Default,
    };

    let sample_rate = config.sample_rate.0;
    let channels = config.channels as usize;

    match sample_format {
        SampleFormat::F32 => build_stream_f32(
            device,
            config,
            channels,
            sample_rate,
            audio_tx,
            stop,
        ),
        SampleFormat::I16 => build_stream_i16(
            device,
            config,
            channels,
            sample_rate,
            audio_tx,
            stop,
        ),
        SampleFormat::U16 => build_stream_u16(
            device,
            config,
            channels,
            sample_rate,
            audio_tx,
            stop,
        ),
        _ => Err("Unsupported sample format".into()),
    }
}

fn build_stream_f32(
    device: cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    sample_rate: u32,
    audio_tx: CbSender<Vec<i16>>,
    stop: Arc<AtomicBool>,
) -> Result<Stream, String> {
    build_stream_with::<f32, _>(
        device,
        config,
        channels,
        sample_rate,
        audio_tx,
        stop,
        |s: &f32| *s,
    )
}

fn build_stream_i16(
    device: cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    sample_rate: u32,
    audio_tx: CbSender<Vec<i16>>,
    stop: Arc<AtomicBool>,
) -> Result<Stream, String> {
    build_stream_with::<i16, _>(
        device,
        config,
        channels,
        sample_rate,
        audio_tx,
        stop,
        |s: &i16| *s as f32 / i16::MAX as f32,
    )
}

fn build_stream_u16(
    device: cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    sample_rate: u32,
    audio_tx: CbSender<Vec<i16>>,
    stop: Arc<AtomicBool>,
) -> Result<Stream, String> {
    build_stream_with::<u16, _>(
        device,
        config,
        channels,
        sample_rate,
        audio_tx,
        stop,
        |s: &u16| {
            let v = *s as f32 / u16::MAX as f32;
            v * 2.0 - 1.0
        },
    )
}

fn build_stream_with<T, F>(
    device: cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    sample_rate: u32,
    audio_tx: CbSender<Vec<i16>>,
    stop: Arc<AtomicBool>,
    convert: F,
) -> Result<Stream, String>
where
    T: cpal::SizedSample + Send + 'static,
    F: Fn(&T) -> f32 + Send + Sync + 'static,
{
    let mut out_buf: Vec<i16> = Vec::with_capacity(CHUNK_SAMPLES * 2);

    let err_fn = move |err| {
        eprintln!("Audio stream error: {err}");
    };

    let stream = device
        .build_input_stream(
            &config,
            move |data: &[T], _| {
                if stop.load(Ordering::Relaxed) {
                    return;
                }

                let mut mono: Vec<f32> =
                    Vec::with_capacity(data.len() / channels);

                for frame in data.chunks(channels) {
                    let mut sum = 0.0f32;
                    for sample in frame {
                        sum += convert(sample);
                    }
                    mono.push(sum / channels as f32);
                }

                let resampled = if sample_rate != TARGET_SAMPLE_RATE {
                    resample_linear(&mono, sample_rate, TARGET_SAMPLE_RATE)
                } else {
                    mono
                };

                for s in resampled {
                    let clipped = s.max(-1.0).min(1.0);
                    let val = (clipped * i16::MAX as f32) as i16;
                    out_buf.push(val);
                }

                while out_buf.len() >= CHUNK_SAMPLES {
                    let chunk: Vec<i16> =
                        out_buf.drain(..CHUNK_SAMPLES).collect();
                    let _ = audio_tx.try_send(chunk);
                }
            },
            err_fn,
            None,
        )
        .map_err(|e: cpal::BuildStreamError| e.to_string())?;

    Ok(stream)
}

fn resample_linear(
    input: &[f32],
    from_rate: u32,
    to_rate: u32,
) -> Vec<f32> {
    if input.is_empty() {
        return Vec::new();
    }

    let ratio = to_rate as f32 / from_rate as f32;
    let out_len = (input.len() as f32 * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);

    for i in 0..out_len {
        let src_pos = i as f32 / ratio;
        let idx = src_pos.floor() as usize;
        let frac = src_pos - idx as f32;

        let a = input[idx];
        let b = input.get(idx + 1).copied().unwrap_or(a);
        out.push(a + (b - a) * frac);
    }

    out
}
