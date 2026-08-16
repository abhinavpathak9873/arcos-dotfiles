use arc_protocol::{socket_path, EventKind, Notification, Request, Response};
use serde_json::{json, Value};
use std::{io::Write as StdWrite, process::Command};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("inspector") {
        Command::new("arc-inspector").spawn()?;
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("waybar") {
        return waybar_watch().await;
    }
    let (method, params) = parse(&args)?;
    let path = std::env::var_os("ARC_CORE_SOCKET")
        .map(Into::into)
        .unwrap_or_else(|| socket_path("arc-core"));
    let mut stream = UnixStream::connect(&path)
        .await
        .map_err(|error| anyhow::anyhow!("cannot connect to {}: {error}", path.display()))?;
    let request = Request {
        jsonrpc: "2.0".into(),
        id: 1,
        method,
        params,
    };
    stream
        .write_all(format!("{}\n", serde_json::to_string(&request)?).as_bytes())
        .await?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).await?;
    let response: Response = serde_json::from_str(&line)?;
    if let Some(error) = response.error {
        anyhow::bail!("{} ({})", error.message, error.code);
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&response.result.unwrap_or(Value::Null))?
    );
    Ok(())
}

async fn waybar_watch() -> anyhow::Result<()> {
    print_waybar(
        "idle",
        "󰚩  Arc",
        "Arc is ready · Caps to talk · Meta+A for recent activity",
    );
    loop {
        let path = std::env::var_os("ARC_CORE_SOCKET")
            .map(Into::into)
            .unwrap_or_else(|| socket_path("arc-core"));
        let Ok(mut stream) = UnixStream::connect(&path).await else {
            print_waybar("error", "󰚩  Arc offline", "Arc core is unavailable");
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            continue;
        };
        let request = Request {
            jsonrpc: "2.0".into(),
            id: 1,
            method: "events/subscribe".into(),
            params: json!({ "topics": ["*"] }),
        };
        stream
            .write_all(format!("{}\n", serde_json::to_string(&request)?).as_bytes())
            .await?;
        let mut lines = BufReader::new(stream).lines();
        while let Some(line) = lines.next_line().await? {
            let Ok(notification) = serde_json::from_str::<Notification>(&line) else {
                continue;
            };
            let event = notification.params;
            match event.kind {
                EventKind::TranscriptProvisional | EventKind::TranscriptStable => {
                    print_waybar("listening", "󰚩  Listening", "Arc is listening locally");
                }
                EventKind::AssistantTextDelta
                | EventKind::TaskProgress
                | EventKind::ModelRouting => {
                    print_waybar("thinking", "󰚩  Working", "Arc is working in the background");
                }
                EventKind::ToolCall => {
                    print_waybar(
                        "controlling",
                        "󰚩  In control",
                        "Arc is using a desktop or system tool",
                    );
                }
                EventKind::DesktopControlState
                    if event.payload.get("active").and_then(Value::as_bool) == Some(true) =>
                {
                    print_waybar(
                        "controlling",
                        "󰚩  In control",
                        "Arc is controlling a desktop surface",
                    );
                }
                EventKind::AssistantTextComplete => {
                    print_waybar("speaking", "󰚩  Speaking", "Arc is responding");
                }
                EventKind::ConfirmationRequested => {
                    print_waybar("attention", "󰚩  Review", "Click to review a confirmation");
                }
                EventKind::SpeechState
                    if event.payload.get("state").and_then(Value::as_str) == Some("idle") =>
                {
                    print_waybar(
                        "idle",
                        "󰚩  Arc",
                        "Arc is ready · Caps to talk · Meta+A for recent activity",
                    );
                }
                EventKind::HardStop => {
                    print_waybar(
                        "idle",
                        "󰚩  Stopped",
                        "All active Arc subsystems were stopped",
                    );
                }
                EventKind::ServiceHealth
                    if event.payload.get("healthy").and_then(Value::as_bool) == Some(false) =>
                {
                    print_waybar(
                        "error",
                        "󰚩  Service issue",
                        "An Arc service needs attention",
                    );
                }
                _ => {}
            }
        }
    }
}

fn print_waybar(class: &str, text: &str, tooltip: &str) {
    println!(
        "{}",
        serde_json::json!({ "text": text, "class": class, "tooltip": tooltip })
    );
    let _ = std::io::stdout().flush();
}

fn parse(args: &[String]) -> anyhow::Result<(String, Value)> {
    let Some(command) = args.first() else {
        return Ok(("health".into(), json!({})));
    };
    let shortcut = match command.as_str() {
        "status" => Some(("health", json!({}))),
        "stop" => Some(("system/stop", json!({}))),
        "voice" if args.get(1).map(String::as_str) == Some("toggle") => {
            Some(("speech/toggle", json!({})))
        }
        "prompt" => Some(("shell/prompt", json!({}))),
        "show" | "toggle" => Some(("shell/toggle", json!({}))),
        "collapse" => Some(("shell/collapse", json!({}))),
        _ => None,
    };
    if let Some(shortcut) = shortcut {
        return Ok((shortcut.0.into(), shortcut.1));
    }
    let params = match args.get(1) {
        Some(raw) => serde_json::from_str(raw)?,
        None => json!({}),
    };
    Ok((command.clone(), params))
}
