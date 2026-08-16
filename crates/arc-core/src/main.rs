use arc_core::service::CoreService;
use arc_protocol::{socket_path, Event, Notification, Request, Response, PROTOCOL_VERSION};
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, LazyLock,
    },
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    sync::{broadcast, Mutex},
};

static ACTIVE_HERMES_SESSIONS: LazyLock<std::sync::Mutex<HashSet<String>>> =
    LazyLock::new(|| std::sync::Mutex::new(HashSet::new()));

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("serve") => {
            let path = args
                .windows(2)
                .find(|pair| pair[0] == "--socket")
                .map(|pair| PathBuf::from(&pair[1]))
                .unwrap_or_else(|| socket_path("arc-core"));
            serve_socket(path).await
        }
        Some("serve-stdio") => serve_stdio().await,
        _ => {
            println!(
                "arc-core {} (protocol {})",
                env!("CARGO_PKG_VERSION"),
                PROTOCOL_VERSION
            );
            Ok(())
        }
    }
}

async fn serve_socket(path: PathBuf) -> anyhow::Result<()> {
    prepare_socket(&path).await?;
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    let _socket_guard = SocketGuard(path.clone());
    let service = Arc::new(Mutex::new(CoreService::open_default()?));
    let (events, _) = broadcast::channel::<Event>(512);
    let sequence = Arc::new(AtomicU64::new(0));
    tokio::spawn(hermes_event_supervisor(
        events.clone(),
        sequence.clone(),
        service.clone(),
    ));

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                if !same_user(&stream)? { continue; }
                let service = service.clone();
                let sender = events.clone();
                let receiver = sender.subscribe();
                let sequence = sequence.clone();
                tokio::spawn(async move {
                    if let Err(error) = serve_client(stream, service, sender, receiver, sequence).await {
                        eprintln!("arc-core client disconnected: {error:#}");
                    }
                });
            }
            signal = tokio::signal::ctrl_c() => { signal?; break; }
        }
    }
    Ok(())
}

async fn serve_client(
    stream: UnixStream,
    service: Arc<Mutex<CoreService>>,
    sender: broadcast::Sender<Event>,
    mut receiver: broadcast::Receiver<Event>,
    sequence: Arc<AtomicU64>,
) -> anyhow::Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    let mut topics = HashSet::<String>::new();
    loop {
        tokio::select! {
            line = lines.next_line() => {
                let Some(line) = line? else { break; };
                let request = match serde_json::from_str::<Request>(&line) {
                    Ok(request) => request,
                    Err(error) => {
                        write_json(&mut writer, &Response::error(0, -32700, error.to_string())).await?;
                        continue;
                    }
                };
                if request.method == "events/subscribe" {
                    topics = subscription_topics(&request.params);
                }
                let method = request.method.clone();
                let request_id = request.id;
                let relay_params = request.params.clone();
                let (mut response, pending) = if method.starts_with("hermes/") {
                    (hermes_request(request_id, &method, relay_params.clone(), &sender, &sequence).await, Vec::new())
                } else {
                    let mut guard = service.lock().await;
                    let response = guard.handle(request);
                    let pending = guard.drain_events();
                    (response, pending)
                };

                if response.error.is_none() {
                    if let Some(relayed) = relay_request(&method, response.id, relay_params).await {
                        response = relayed;
                    }
                }
                if let Some((utterance_id, text)) = stable_transcript(&method, &response) {
                    let service = service.clone();
                    let sender = sender.clone();
                    let sequence = sequence.clone();
                    tokio::spawn(async move { submit_native_utterance(service, sender, sequence, utterance_id, text).await; });
                }
                if method == "conversation/submit" && accepted_once(&response) {
                    let accepted = response.result.clone().unwrap_or(Value::Null);
                    let sender = sender.clone();
                    let sequence = sequence.clone();
                    tokio::spawn(async move { forward_accepted_to_hermes(accepted, sender, sequence).await; });
                }
                write_json(&mut writer, &response).await?;
                for pending_event in pending {
                    let event = Event {
                        sequence: sequence.fetch_add(1, Ordering::Relaxed) + 1,
                        at: Utc::now().to_rfc3339(),
                        kind: pending_event.kind,
                        payload: pending_event.payload,
                    };
                    let _ = sender.send(event);
                }
                if method == "system/stop" || method == "turn/cancel" {
                    tokio::spawn(hard_stop_dependents());
                }
            }
            event = receiver.recv(), if !topics.is_empty() => {
                match event {
                    Ok(event) if topics.contains("*") || topics.contains(event.kind.topic()) => {
                        write_json(&mut writer, &Notification::event(event)).await?;
                    }
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
    Ok(())
}

fn accepted_once(response: &Response) -> bool {
    response.error.is_none()
        && response
            .result
            .as_ref()
            .and_then(|value| value.get("accepted"))
            .and_then(Value::as_bool)
            == Some(true)
        && response
            .result
            .as_ref()
            .and_then(|value| value.get("duplicate"))
            .and_then(Value::as_bool)
            == Some(false)
}

fn stable_transcript(method: &str, response: &Response) -> Option<(uuid::Uuid, String)> {
    if !matches!(method, "speech/finish" | "speech/toggle") {
        return None;
    }
    let result = response.result.as_ref()?;
    if result.get("stable").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let id = result
        .get("utteranceId")
        .and_then(Value::as_str)?
        .parse()
        .ok()?;
    let text = result
        .get("text")
        .and_then(Value::as_str)?
        .trim()
        .to_owned();
    (!text.is_empty()).then_some((id, text))
}

async fn submit_native_utterance(
    service: Arc<Mutex<CoreService>>,
    events: broadcast::Sender<Event>,
    sequence: Arc<AtomicU64>,
    utterance_id: uuid::Uuid,
    text: String,
) {
    let request = Request {
        jsonrpc: "2.0".into(),
        id: 0,
        method: "conversation/submit".into(),
        params: serde_json::json!({ "utteranceId": utterance_id, "text": text }),
    };
    let mut service = service.lock().await;
    let accepted = service.handle(request);
    let pending = service.drain_events();
    drop(service);
    let duplicate = accepted
        .result
        .as_ref()
        .and_then(|value| value.get("duplicate"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    for pending_event in pending {
        let _ = events.send(Event {
            sequence: sequence.fetch_add(1, Ordering::Relaxed) + 1,
            at: Utc::now().to_rfc3339(),
            kind: pending_event.kind,
            payload: pending_event.payload,
        });
    }
    let _ = events.send(Event {
        sequence: sequence.fetch_add(1, Ordering::Relaxed) + 1,
        at: Utc::now().to_rfc3339(),
        kind: arc_protocol::EventKind::TranscriptStable,
        payload: serde_json::json!({ "utteranceId": utterance_id, "text": text }),
    });
    if duplicate {
        return;
    }
    forward_accepted_to_hermes(accepted.result.unwrap_or(Value::Null), events, sequence).await;
}

async fn forward_accepted_to_hermes(
    accepted: Value,
    events: broadcast::Sender<Event>,
    sequence: Arc<AtomicU64>,
) {
    let Some(utterance) = accepted.get("utterance") else {
        return;
    };
    let route = utterance.get("route").cloned().unwrap_or(Value::Null);
    let text = utterance
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let existing_session = utterance
        .get("hermes_session_id")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let task_id = utterance
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let room_id = utterance.get("room_id").cloned().unwrap_or(Value::Null);
    let cwd = std::env::var("ARC_WORKSPACE").unwrap_or_else(|_| {
        std::env::var("HOME")
            .map(|home| format!("{home}/Projects"))
            .unwrap_or_else(|_| ".".into())
    });
    let codex_reference = if route.get("delegate_to_codex").and_then(Value::as_bool) == Some(true) {
        let params = serde_json::json!({ "taskId": task_id, "roomId": room_id, "cwd": cwd, "prompt": text, "model": route.get("model"), "effort": route.get("effort") });
        relay_request("codex/delegate", 1, params)
            .await
            .and_then(|response| response.result)
            .and_then(|result| result.get("reference").cloned())
    } else {
        None
    };
    if let Some(reference) = &codex_reference {
        let _ = events.send(Event { sequence: sequence.fetch_add(1, Ordering::Relaxed) + 1, at: Utc::now().to_rfc3339(), kind: arc_protocol::EventKind::TaskProgress, payload: serde_json::json!({ "taskId": task_id, "state": "delegated", "codex": reference }) });
    }
    let session_id = if let Some(session_id) = existing_session {
        session_id
    } else {
        let session = hermes_websocket("session.create", serde_json::json!({ "cwd": cwd, "model": route.get("model"), "reasoning_effort": route.get("effort"), "source": "arc-shell" }), &events, &sequence).await;
        let Ok(session) = session else {
            let _ = events.send(Event {
                sequence: sequence.fetch_add(1, Ordering::Relaxed) + 1,
                at: Utc::now().to_rfc3339(),
                kind: arc_protocol::EventKind::ServiceHealth,
                payload: serde_json::json!({ "service": "hermes", "healthy": false }),
            });
            return;
        };
        let Some(session_id) = session.get("session_id").and_then(Value::as_str) else {
            return;
        };
        session_id.to_owned()
    };
    let _ = hermes_websocket("config.set", serde_json::json!({ "session_id": session_id, "key": "model", "value": format!("{} --provider openai-codex --session", route.get("model").and_then(Value::as_str).unwrap_or("gpt-5.6-luna")) }), &events, &sequence).await;
    let _ = hermes_websocket("config.set", serde_json::json!({ "session_id": session_id, "key": "reasoning", "value": route.get("effort") }), &events, &sequence).await;
    let coordinator_text = if let Some(reference) = codex_reference {
        format!("The user requested: {text}\nArc delegated repository execution to Codex with reference {reference}. Remain the conversational coordinator: report progress and do not repeat the same repository edits yourself.")
    } else {
        text
    };
    ACTIVE_HERMES_SESSIONS
        .lock()
        .expect("active Hermes session lock")
        .insert(session_id.clone());
    let _ = hermes_websocket(
        "prompt.submit",
        serde_json::json!({ "session_id": session_id, "text": coordinator_text }),
        &events,
        &sequence,
    )
    .await;
}

async fn relay_request(method: &str, id: u64, params: Value) -> Option<Response> {
    let target = if matches!(
        method,
        "speech/start"
            | "speech/finish"
            | "speech/toggle"
            | "speech/cancel"
            | "speech/speak"
            | "speech/sanitize"
            | "speech/configure"
    ) {
        "arc-speech"
    } else if method.starts_with("codex/") && method != "codex/reportEvent" {
        "arc-codex"
    } else {
        return None;
    };
    let path = socket_path(target);
    let mut stream = match UnixStream::connect(&path).await {
        Ok(stream) => stream,
        Err(_) if target == "arc-codex" => {
            let _ = tokio::process::Command::new("systemctl")
                .args(["--user", "start", "arc-codex.service"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .await;
            let mut connected = None;
            for _ in 0..40 {
                if let Ok(stream) = UnixStream::connect(&path).await {
                    connected = Some(stream);
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
            connected?
        }
        Err(_) => return None,
    };
    let request = Request {
        jsonrpc: "2.0".into(),
        id,
        method: method.into(),
        params,
    };
    stream
        .write_all(format!("{}\n", serde_json::to_string(&request).ok()?).as_bytes())
        .await
        .ok()?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).await.ok()?;
    serde_json::from_str(&line).ok()
}

async fn hermes_request(
    id: u64,
    method: &str,
    params: Value,
    events: &broadcast::Sender<Event>,
    sequence: &AtomicU64,
) -> Response {
    let result = if method == "hermes/http" {
        hermes_http(params).await
    } else {
        let target_method = if method == "hermes/request" {
            params
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        } else {
            method.trim_start_matches("hermes/").replace('_', ".")
        };
        let target_params = if method == "hermes/request" {
            params
                .get("params")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}))
        } else {
            params
        };
        hermes_websocket(&target_method, target_params, events, sequence).await
    };
    match result {
        Ok(value) => Response::ok(id, value),
        Err(error) => Response::error(id, -32020, format!("Hermes unavailable: {error}")),
    }
}

async fn hermes_websocket(
    method: &str,
    params: Value,
    _events: &broadcast::Sender<Event>,
    _sequence: &AtomicU64,
) -> anyhow::Result<Value> {
    let url = hermes_websocket_url()?;
    let (mut socket, _) = tokio_tungstenite::connect_async(&url).await?;
    socket.send(tokio_tungstenite::tungstenite::Message::Text(serde_json::to_string(&serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }))?.into())).await?;
    while let Some(message) = socket.next().await {
        let message = message?;
        let Some(text) = (match message {
            tokio_tungstenite::tungstenite::Message::Text(text) => Some(text),
            _ => None,
        }) else {
            continue;
        };
        let value: Value = serde_json::from_str(&text)?;
        if value.get("id").and_then(Value::as_u64) == Some(1) {
            if let Some(error) = value.get("error") {
                anyhow::bail!("{error}");
            }
            return Ok(value.get("result").cloned().unwrap_or(Value::Null));
        }
    }
    anyhow::bail!("connection closed before a response")
}

fn hermes_websocket_url() -> anyhow::Result<String> {
    let base =
        std::env::var("ARC_HERMES_WS").unwrap_or_else(|_| "ws://127.0.0.1:43826/api/ws".into());
    let token = hermes_token()?;
    let separator = if base.contains('?') { '&' } else { '?' };
    Ok(format!(
        "{base}{separator}token={}",
        urlencoding::encode(&token)
    ))
}

#[derive(Default)]
struct StreamedSpeech {
    buffer: String,
    saw_delta: bool,
}

async fn hermes_event_supervisor(
    events: broadcast::Sender<Event>,
    sequence: Arc<AtomicU64>,
    service: Arc<Mutex<CoreService>>,
) {
    loop {
        if let Err(error) = hermes_event_connection(&events, &sequence, &service).await {
            publish_event(
                &events,
                &sequence,
                arc_protocol::EventKind::ServiceHealth,
                serde_json::json!({ "service": "hermes", "healthy": false, "error": error.to_string() }),
            );
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

async fn hermes_event_connection(
    events: &broadcast::Sender<Event>,
    sequence: &AtomicU64,
    service: &Arc<Mutex<CoreService>>,
) -> anyhow::Result<()> {
    let (mut socket, _) = tokio_tungstenite::connect_async(hermes_websocket_url()?).await?;
    publish_event(
        events,
        sequence,
        arc_protocol::EventKind::ServiceHealth,
        serde_json::json!({ "service": "hermes", "healthy": true }),
    );
    let mut speech = HashMap::<String, StreamedSpeech>::new();
    while let Some(message) = socket.next().await {
        let message = message?;
        let tokio_tungstenite::tungstenite::Message::Text(text) = message else {
            continue;
        };
        let value: Value = serde_json::from_str(&text)?;
        if value.get("method").and_then(Value::as_str) != Some("event") {
            continue;
        }
        let Some(hermes) = value.get("params") else {
            continue;
        };
        let event_type = hermes
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let kind = match event_type {
            "message.delta" => arc_protocol::EventKind::AssistantTextDelta,
            "message.complete" => arc_protocol::EventKind::AssistantTextComplete,
            value if value.starts_with("tool") => arc_protocol::EventKind::ToolCall,
            _ => arc_protocol::EventKind::TaskProgress,
        };
        {
            let projected = serde_json::json!({ "hermes": hermes });
            service
                .lock()
                .await
                .record_external_event(&kind, &projected);
        }
        publish_event(
            events,
            sequence,
            kind,
            serde_json::json!({ "hermes": hermes }),
        );
        schedule_streamed_speech(hermes, &mut speech).await;
    }
    anyhow::bail!("Hermes event stream closed")
}

async fn schedule_streamed_speech(hermes: &Value, streams: &mut HashMap<String, StreamedSpeech>) {
    let event_type = hermes
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let session = hermes
        .get("session_id")
        .and_then(Value::as_str)
        .unwrap_or("default")
        .to_owned();
    match event_type {
        "message.start" => {
            streams.insert(session, StreamedSpeech::default());
        }
        "message.delta" => {
            let Some(delta) = hermes_event_text(hermes) else {
                return;
            };
            let stream = streams.entry(session).or_default();
            stream.saw_delta = true;
            stream.buffer.push_str(&delta);
            let (sentences, remainder) = arc_speech::take_complete_sentences(&stream.buffer, false);
            stream.buffer = remainder;
            if !sentences.is_empty() {
                queue_speech(sentences.join(" ")).await;
            }
        }
        "message.complete" => {
            let stream = streams.remove(&session).unwrap_or_default();
            ACTIVE_HERMES_SESSIONS
                .lock()
                .expect("active Hermes session lock")
                .remove(&session);
            let text = if stream.saw_delta {
                stream.buffer
            } else {
                hermes_event_text(hermes).unwrap_or_default()
            };
            if !text.trim().is_empty() {
                queue_speech(text).await;
            }
        }
        _ => {}
    }
}

fn hermes_event_text(hermes: &Value) -> Option<String> {
    let payload = hermes.get("payload").unwrap_or(hermes);
    ["text", "delta", "message"]
        .iter()
        .find_map(|key| payload.get(key).and_then(Value::as_str))
        .map(str::to_owned)
}

async fn queue_speech(text: String) {
    let _ = relay_request(
        "speech/speak",
        0,
        serde_json::json!({ "text": text, "flush": true }),
    )
    .await;
}

fn publish_event(
    events: &broadcast::Sender<Event>,
    sequence: &AtomicU64,
    kind: arc_protocol::EventKind,
    payload: Value,
) {
    let _ = events.send(Event {
        sequence: sequence.fetch_add(1, Ordering::Relaxed) + 1,
        at: Utc::now().to_rfc3339(),
        kind,
        payload,
    });
}

async fn hermes_http(params: Value) -> anyhow::Result<Value> {
    let path = params
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("path is required"))?;
    anyhow::ensure!(
        path.starts_with("/api/"),
        "only Hermes API paths are allowed"
    );
    let method = params
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("GET")
        .parse::<reqwest::Method>()?;
    let base = std::env::var("ARC_HERMES_HTTP").unwrap_or_else(|_| "http://127.0.0.1:43826".into());
    let client = reqwest::Client::new();
    let mut request = client
        .request(method, format!("{base}{path}"))
        .bearer_auth(hermes_token()?);
    if let Some(body) = params.get("body") {
        request = request.json(body);
    }
    let response = request.send().await?;
    let status = response.status();
    let bytes = response.bytes().await?;
    anyhow::ensure!(
        status.is_success(),
        "HTTP {status}: {}",
        String::from_utf8_lossy(&bytes)
    );
    Ok(if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)?
    })
}

fn hermes_token() -> anyhow::Result<String> {
    if let Ok(token) = std::env::var("ARC_HERMES_TOKEN") {
        return Ok(token);
    }
    let path = std::env::var_os("ARC_HERMES_TOKEN_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| arc_protocol::runtime_dir().join("hermes.token"));
    Ok(std::fs::read_to_string(path)?.trim().to_owned())
}

async fn hard_stop_dependents() {
    let request = Request {
        jsonrpc: "2.0".into(),
        id: 1,
        method: "system/stop".into(),
        params: Value::Object(Default::default()),
    };
    let bytes = format!("{}\n", serde_json::to_string(&request).unwrap_or_default());
    let speech = async {
        if let Ok(mut stream) = UnixStream::connect(socket_path("arc-speech")).await {
            let _ = stream.write_all(bytes.as_bytes()).await;
        }
    };
    let codex = async {
        if let Ok(mut stream) = UnixStream::connect(socket_path("arc-codex")).await {
            let _ = stream.write_all(bytes.as_bytes()).await;
        }
    };
    let hermes_sessions = ACTIVE_HERMES_SESSIONS
        .lock()
        .expect("active Hermes session lock")
        .drain()
        .collect::<Vec<_>>();
    let hermes = async {
        for session_id in hermes_sessions {
            let (events, _) = broadcast::channel(1);
            let sequence = AtomicU64::new(0);
            let _ = hermes_websocket(
                "session.interrupt",
                serde_json::json!({ "session_id": session_id }),
                &events,
                &sequence,
            )
            .await;
        }
    };
    let systemd = async {
        let _ = tokio::process::Command::new("systemctl")
            .args(["--user", "stop", "arcos-agent.target"])
            .status()
            .await;
    };
    tokio::join!(speech, codex, hermes, systemd);
}

fn subscription_topics(params: &Value) -> HashSet<String> {
    params
        .get("topics")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_else(|| HashSet::from(["*".into()]))
}

async fn prepare_socket(path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
    }
    if path.exists() {
        if UnixStream::connect(path).await.is_ok() {
            anyhow::bail!("arc-core is already listening at {}", path.display());
        }
        std::fs::remove_file(path)?;
    }
    Ok(())
}

fn same_user(stream: &UnixStream) -> std::io::Result<bool> {
    let peer = stream.peer_cred()?;
    let own_uid = std::fs::metadata("/proc/self")?.uid();
    Ok(peer.uid() == own_uid)
}

async fn write_json(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    value: &impl serde::Serialize,
) -> anyhow::Result<()> {
    writer
        .write_all(format!("{}\n", serde_json::to_string(value)?).as_bytes())
        .await?;
    Ok(())
}

async fn serve_stdio() -> anyhow::Result<()> {
    let mut service = CoreService::open_default()?;
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();
    while let Some(line) = lines.next_line().await? {
        let response = match serde_json::from_str::<Request>(&line) {
            Ok(request) => service.handle(request),
            Err(error) => Response::error(0, -32700, error.to_string()),
        };
        stdout
            .write_all(format!("{}\n", serde_json::to_string(&response)?).as_bytes())
            .await?;
        stdout.flush().await?;
    }
    Ok(())
}

struct SocketGuard(PathBuf);
impl Drop for SocketGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
