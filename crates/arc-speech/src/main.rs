use arc_protocol::{socket_path, Request, Response};
use arc_speech::{sanitize_for_speech, take_complete_sentences};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::PathBuf,
    process::Stdio,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    process::{Child, Command},
    sync::{mpsc, watch},
    time::{timeout, Duration},
};
use uuid::Uuid;

struct SpeechState {
    capture: Option<Capture>,
    playback: Option<Child>,
    completed: HashMap<Uuid, String>,
    tts_tx: mpsc::UnboundedSender<TtsJob>,
    tts_generation: watch::Sender<u64>,
    config: SpeechConfig,
    config_path: PathBuf,
}

struct TtsJob {
    generation: u64,
    text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct SpeechConfig {
    enabled: bool,
    speed: f64,
    voice: String,
}

impl Default for SpeechConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            speed: 1.0,
            voice: "bm_fable".into(),
        }
    }
}

struct Capture {
    id: Uuid,
    path: PathBuf,
    process: Child,
    stop: tokio::sync::watch::Sender<bool>,
    monitor: tokio::task::JoinHandle<()>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if std::env::args().nth(1).as_deref() != Some("serve") {
        println!("arc-speech {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let path = socket_path("arc-speech");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
    }
    if path.exists() {
        if UnixStream::connect(&path).await.is_ok() {
            anyhow::bail!("arc-speech is already running");
        }
        std::fs::remove_file(&path)?;
    }
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    let state_dir = std::env::var_os("ARC_STATE_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_STATE_HOME").map(|path| PathBuf::from(path).join("arc")))
        .or_else(|| {
            std::env::var_os("HOME").map(|path| PathBuf::from(path).join(".local/state/arc"))
        })
        .unwrap_or_else(|| PathBuf::from(".arc-state"));
    std::fs::create_dir_all(&state_dir)?;
    let config_path = state_dir.join("speech.json");
    let config = std::fs::read(&config_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let (tts_tx, tts_rx) = mpsc::unbounded_channel();
    let (tts_generation, _) = watch::channel(0_u64);
    let state = std::sync::Arc::new(tokio::sync::Mutex::new(SpeechState {
        capture: None,
        playback: None,
        completed: HashMap::new(),
        tts_tx,
        tts_generation,
        config,
        config_path,
    }));
    tokio::spawn(tts_worker(state.clone(), tts_rx));
    loop {
        let (stream, _) = listener.accept().await?;
        if stream.peer_cred()?.uid() != std::fs::metadata("/proc/self")?.uid() {
            continue;
        }
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(error) = client(stream, state).await {
                eprintln!("arc-speech client: {error:#}");
            }
        });
    }
}

async fn client(
    stream: UnixStream,
    state: std::sync::Arc<tokio::sync::Mutex<SpeechState>>,
) -> anyhow::Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await? {
        let request = match serde_json::from_str::<Request>(&line) {
            Ok(value) => value,
            Err(error) => {
                write(&mut writer, Response::error(0, -32700, error.to_string())).await?;
                continue;
            }
        };
        let id = request.id;
        let result = dispatch(request, &state).await;
        let response = match result {
            Ok(value) => Response::ok(id, value),
            Err(error) => Response::error(id, -32000, error.to_string()),
        };
        write(&mut writer, response).await?;
    }
    Ok(())
}

async fn dispatch(
    request: Request,
    state: &tokio::sync::Mutex<SpeechState>,
) -> anyhow::Result<Value> {
    match request.method.as_str() {
        "health" => {
            let state = state.lock().await;
            Ok(
                json!({ "status": "ready", "listening": state.capture.is_some(), "speaking": state.playback.is_some(), "ttsEnabled": state.config.enabled, "engine": "pipewire+silero-vad+whisper.cpp" }),
            )
        }
        "speech/start" => {
            #[derive(Deserialize, Default)]
            #[serde(rename_all = "camelCase")]
            struct Params {
                utterance_id: Option<Uuid>,
            }
            let params: Params = serde_json::from_value(request.params).unwrap_or_default();
            start_capture(state, params.utterance_id).await
        }
        "speech/finish" => finish_capture(state).await,
        "speech/toggle" => {
            if state.lock().await.capture.is_some() {
                finish_capture(state).await
            } else {
                start_capture(state, None).await
            }
        }
        "speech/cancel" | "system/stop" => {
            let mut state = state.lock().await;
            if let Some(mut capture) = state.capture.take() {
                let _ = capture.stop.send(true);
                capture.monitor.abort();
                let _ = capture.process.start_kill();
                let _ = std::fs::remove_file(capture.path);
            }
            if let Some(mut playback) = state.playback.take() {
                let _ = playback.start_kill();
            }
            let generation = *state.tts_generation.borrow() + 1;
            let _ = state.tts_generation.send(generation);
            Ok(json!({ "cancelled": true, "state": "idle" }))
        }
        "speech/configure" => {
            #[derive(Deserialize, Default)]
            struct Params {
                enabled: Option<bool>,
                speed: Option<f64>,
                voice: Option<String>,
            }
            let params: Params = serde_json::from_value(request.params)?;
            let mut state = state.lock().await;
            if let Some(enabled) = params.enabled {
                state.config.enabled = enabled;
            }
            if let Some(speed) = params.speed {
                state.config.speed = speed.clamp(0.5, 2.0);
            }
            if let Some(voice) = params.voice {
                state.config.voice = voice;
            }
            persist_config(&state.config_path, &state.config)?;
            Ok(
                json!({ "enabled": state.config.enabled, "speed": state.config.speed, "voice": state.config.voice }),
            )
        }
        "speech/sanitize" => {
            let text = request
                .params
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default();
            Ok(json!({ "text": sanitize_for_speech(text) }))
        }
        "speech/speak" => {
            let text = request
                .params
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let flush = request
                .params
                .get("flush")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let sanitized = sanitize_for_speech(text);
            let (sentences, remainder) = take_complete_sentences(&sanitized, flush);
            let text = sentences.join(" ");
            let state = state.lock().await;
            let scheduled = state.config.enabled && !text.is_empty();
            if scheduled {
                state
                    .tts_tx
                    .send(TtsJob {
                        generation: *state.tts_generation.borrow(),
                        text,
                    })
                    .map_err(|_| anyhow::anyhow!("TTS scheduler stopped"))?;
            }
            Ok(json!({ "scheduled": scheduled, "spoken": sentences, "remainder": remainder }))
        }
        _ => anyhow::bail!("method not found"),
    }
}

async fn tts_worker(
    state: std::sync::Arc<tokio::sync::Mutex<SpeechState>>,
    mut jobs: mpsc::UnboundedReceiver<TtsJob>,
) {
    while let Some(job) = jobs.recv().await {
        let should_speak = {
            let state = state.lock().await;
            state.config.enabled && *state.tts_generation.borrow() == job.generation
        };
        if !should_speak {
            continue;
        }
        report_speech_state("speaking", Some(&job.text)).await;
        if let Err(error) = play(&state, &job.text).await {
            eprintln!("arc-speech TTS: {error:#}");
        }
        report_speech_state("idle", None).await;
    }
}

async fn play(state: &tokio::sync::Mutex<SpeechState>, text: &str) -> anyhow::Result<()> {
    let program = std::env::var("ARC_TTS_COMMAND").unwrap_or_else(|_| "espeak-ng".into());
    let (speed, voice) = {
        let state = state.lock().await;
        (
            std::env::var("ARC_TTS_SPEED")
                .unwrap_or_else(|_| format!("{:.0}", 168.0 * state.config.speed)),
            espeak_voice(&state.config.voice),
        )
    };
    let child = Command::new(program)
        .args(["-s", &speed, "-v", voice, text])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    state.lock().await.playback = Some(child);
    loop {
        let status = {
            let mut guard = state.lock().await;
            match guard.playback.as_mut() {
                Some(child) => child.try_wait()?,
                None => return Ok(()),
            }
        };
        if let Some(status) = status {
            state.lock().await.playback.take();
            anyhow::ensure!(status.success(), "local TTS failed");
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

fn espeak_voice(id: &str) -> &'static str {
    match id {
        "bm_george" | "bm_fable" => "en-gb",
        "bf_emma" => "en-gb+f3",
        "af_heart" => "en-us+f3",
        "af_bella" => "en-us+f4",
        "am_fenrir" => "en-us+m3",
        "am_michael" => "en-us+m2",
        _ => "en-us",
    }
}

fn persist_config(path: &PathBuf, config: &SpeechConfig) -> anyhow::Result<()> {
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, serde_json::to_vec_pretty(config)?)?;
    std::fs::rename(temporary, path)?;
    Ok(())
}

async fn start_capture(
    state: &tokio::sync::Mutex<SpeechState>,
    utterance_id: Option<Uuid>,
) -> anyhow::Result<Value> {
    let mut state = state.lock().await;
    if let Some(capture) = &state.capture {
        return Ok(json!({ "utteranceId": capture.id, "state": "listening", "duplicate": true }));
    }
    let id = utterance_id.unwrap_or_else(Uuid::new_v4);
    let path = arc_protocol::runtime_dir().join(format!("utterance-{id}.wav"));
    let program = std::env::var("ARC_PIPEWIRE_RECORD").unwrap_or_else(|_| "pw-record".into());
    let mut command = Command::new(program);
    command.args(["--format=s16", "--rate=16000", "--channels=1"]);
    if let Some(target) = std::env::var("ARC_PIPEWIRE_TARGET")
        .ok()
        .filter(|target| !target.trim().is_empty())
    {
        command.arg("--target").arg(target);
    }
    let process = command
        .arg(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let (stop, stop_rx) = tokio::sync::watch::channel(false);
    let monitor = tokio::spawn(monitor_capture(id, path.clone(), stop_rx));
    state.capture = Some(Capture {
        id,
        path,
        process,
        stop,
        monitor,
    });
    Ok(json!({ "utteranceId": id, "state": "listening" }))
}

async fn finish_capture(state: &tokio::sync::Mutex<SpeechState>) -> anyhow::Result<Value> {
    let mut capture = {
        let mut state = state.lock().await;
        let Some(capture) = state.capture.take() else {
            return Ok(json!({ "state": "idle", "submitted": false }));
        };
        capture
    };
    let _ = capture.stop.send(true);
    let _ = capture.monitor.await;
    finalize_capture(&mut capture.process).await?;
    if let Some(text) = state.lock().await.completed.get(&capture.id).cloned() {
        return Ok(
            json!({ "utteranceId": capture.id, "text": text, "stable": true, "duplicate": true }),
        );
    }
    let decoded = async {
        let speech = detect_speech(capture.path.clone()).await?;
        if speech {
            transcribe(&capture.path, "final").await
        } else {
            Ok(String::new())
        }
    }
    .await;
    let _ = std::fs::remove_file(&capture.path);
    let text = decoded?;
    state
        .lock()
        .await
        .completed
        .insert(capture.id, text.clone());
    Ok(json!({ "utteranceId": capture.id, "text": text, "stable": true, "duplicate": false }))
}

async fn finalize_capture(process: &mut Child) -> anyhow::Result<()> {
    if process.try_wait()?.is_some() {
        return Ok(());
    }
    let pid = process
        .id()
        .ok_or_else(|| anyhow::anyhow!("PipeWire capture has no process id"))?;
    // SAFETY: `pid` belongs to the live child above, and SIGINT only asks
    // pw-record to close its WAV stream and update the container header.
    let result = unsafe { libc::kill(pid as i32, libc::SIGINT) };
    if result != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    if timeout(Duration::from_secs(3), process.wait())
        .await
        .is_err()
    {
        process.start_kill()?;
        let _ = process.wait().await;
        anyhow::bail!("PipeWire capture did not stop cleanly");
    }
    Ok(())
}

async fn monitor_capture(id: Uuid, path: PathBuf, mut stop: tokio::sync::watch::Receiver<bool>) {
    let mut interval = tokio::time::interval(std::time::Duration::from_millis(1400));
    interval.tick().await;
    loop {
        tokio::select! {
            _ = interval.tick() => {
                if detect_speech(path.clone()).await.unwrap_or(false) {
                    if let Ok(text) = transcribe(&path, "partial").await { if !text.is_empty() { report_transcript(id, &text, false).await; } }
                }
            }
            changed = stop.changed() => { if changed.is_err() || *stop.borrow() { break; } }
        }
    }
}

async fn detect_speech(path: PathBuf) -> anyhow::Result<bool> {
    tokio::task::spawn_blocking(move || {
        use silero_vad_rust::{get_speech_timestamps, load_silero_vad, read_audio};
        let audio = read_audio(&path, 16_000)?;
        if audio.len() < 512 {
            return Ok(false);
        }
        let mut model = if let Some(path) = std::env::var("ARC_SILERO_MODEL")
            .ok()
            .filter(|path| !path.trim().is_empty())
        {
            silero_vad_rust::silero_vad::model::OnnxModel::from_path(path, true)?
        } else {
            load_silero_vad()?
        };
        let segments = get_speech_timestamps(&audio, &mut model, &Default::default())?;
        Ok::<bool, anyhow::Error>(!segments.is_empty())
    })
    .await?
}

async fn report_transcript(id: Uuid, text: &str, stable: bool) {
    if let Ok(mut stream) = UnixStream::connect(socket_path("arc-core")).await {
        let request = Request {
            jsonrpc: "2.0".into(),
            id: 1,
            method: "speech/reportTranscript".into(),
            params: json!({ "utteranceId": id, "text": text, "stable": stable }),
        };
        let _ = stream
            .write_all(
                format!("{}\n", serde_json::to_string(&request).unwrap_or_default()).as_bytes(),
            )
            .await;
    }
}

async fn report_speech_state(state: &str, text: Option<&str>) {
    if let Ok(mut stream) = UnixStream::connect(socket_path("arc-core")).await {
        let request = Request {
            jsonrpc: "2.0".into(),
            id: 1,
            method: "speech/reportState".into(),
            params: json!({ "state": state, "text": text }),
        };
        let _ = stream
            .write_all(
                format!("{}\n", serde_json::to_string(&request).unwrap_or_default()).as_bytes(),
            )
            .await;
    }
}

async fn transcribe(path: &PathBuf, phase: &str) -> anyhow::Result<String> {
    let whisper = std::env::var("ARC_WHISPER_CPP").unwrap_or_else(|_| "whisper-cli".into());
    let model = std::env::var("ARC_WHISPER_MODEL")
        .map_err(|_| anyhow::anyhow!("ARC_WHISPER_MODEL is not configured"))?;
    let prefix = path.with_extension(format!("transcript-{phase}"));
    let status = Command::new(whisper)
        .args(["--model", &model, "--file"])
        .arg(path)
        .args(["--output-txt", "--output-file"])
        .arg(&prefix)
        .args(["--no-timestamps"])
        .status()
        .await?;
    anyhow::ensure!(status.success(), "whisper.cpp decoding failed");
    let transcript_path = PathBuf::from(format!("{}.txt", prefix.display()));
    let text = tokio::fs::read_to_string(&transcript_path).await?;
    let _ = std::fs::remove_file(transcript_path);
    Ok(text.trim().to_owned())
}

async fn write(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    response: Response,
) -> anyhow::Result<()> {
    writer
        .write_all(format!("{}\n", serde_json::to_string(&response)?).as_bytes())
        .await?;
    Ok(())
}
