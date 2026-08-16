use arc_protocol::{WindowDescriptor, WorkspaceDescriptor};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::{
    env,
    process::{Command, Stdio},
};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FocusedContext {
    pub application: Option<String>,
    pub process: Option<String>,
    pub document_title: Option<String>,
    pub selected_text: Option<String>,
    pub text_around_caret: Option<String>,
    pub working_directory: Option<PathBuf>,
    pub git_root: Option<PathBuf>,
    pub browser_url: Option<String>,
    pub clipboard_mime_types: Vec<String>,
    pub workspace: Option<String>,
    pub output: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Application {
    pub id: String,
    pub name: String,
}

#[derive(Debug, thiserror::Error)]
pub enum DesktopError {
    #[error("permission not granted: {0}")]
    Permission(String),
    #[error("desktop capability unavailable: {0}")]
    Unavailable(String),
    #[error("desktop operation failed: {0}")]
    Operation(String),
}

/// The ArcOS desktop boundary. Its only supported implementation targets Sway
/// and composes Sway IPC with the native Linux control paths used by the core.
#[async_trait]
pub trait DesktopAdapter: Send + Sync {
    async fn focused_context(&self) -> Result<FocusedContext, DesktopError>;
    async fn capture_screen(&self) -> Result<Vec<u8>, DesktopError>;
    async fn list_applications(&self) -> Result<Vec<Application>, DesktopError>;
    async fn launch_application(&self, id: &str) -> Result<(), DesktopError>;
    async fn move_pointer(&self, point: Point) -> Result<(), DesktopError>;
    async fn click(&self, button: u8) -> Result<(), DesktopError>;
    async fn type_text(&self, text: &str) -> Result<(), DesktopError>;
    async fn press_keys(&self, keys: &[String]) -> Result<(), DesktopError>;
    async fn read_clipboard(&self) -> Result<String, DesktopError>;
    async fn write_clipboard(&self, value: &str) -> Result<(), DesktopError>;
    async fn stop_control(&self) -> Result<(), DesktopError>;
}

/// ArcOS supports one compositor, so its desktop implementation speaks Sway
/// IPC directly. Commands are argument-vector based and never pass user text
/// through a shell.
#[derive(Debug, Clone)]
pub struct SwayAdapter {
    swaymsg: String,
}

impl Default for SwayAdapter {
    fn default() -> Self {
        Self {
            swaymsg: env::var("ARC_SWAYMSG").unwrap_or_else(|_| "swaymsg".into()),
        }
    }
}

impl SwayAdapter {
    pub fn windows(&self) -> Result<Vec<WindowDescriptor>, DesktopError> {
        let tree = self.query("get_tree")?;
        let mut windows = Vec::new();
        collect_windows(&tree, None, &mut windows);
        Ok(windows)
    }

    pub fn workspaces(&self) -> Result<Vec<WorkspaceDescriptor>, DesktopError> {
        let values = self.query("get_workspaces")?;
        serde_json::from_value(values).map_err(|error| DesktopError::Operation(error.to_string()))
    }

    pub fn outputs(&self) -> Result<Value, DesktopError> {
        self.query("get_outputs")
    }

    pub fn focus_window(&self, id: i64) -> Result<bool, DesktopError> {
        self.command(&format!("[con_id={id}] focus"))?;
        Ok(self
            .windows()?
            .iter()
            .any(|window| window.id == id && window.focused))
    }

    pub fn move_window(&self, id: i64, workspace: &str) -> Result<bool, DesktopError> {
        self.command(&format!(
            "[con_id={id}] move container to workspace {workspace}"
        ))?;
        Ok(self
            .windows()?
            .iter()
            .any(|window| window.id == id && window.workspace.as_deref() == Some(workspace)))
    }

    pub fn focus_workspace(&self, workspace: &str) -> Result<bool, DesktopError> {
        self.command(&format!("workspace {workspace}"))?;
        Ok(self
            .workspaces()?
            .iter()
            .any(|item| item.name == workspace && item.focused))
    }

    pub fn move_agent_pointer(&self, point: Point) -> Result<bool, DesktopError> {
        if !point.x.is_finite() || !point.y.is_finite() || point.x < 0.0 || point.y < 0.0 {
            return Err(DesktopError::Operation(
                "pointer coordinates must be finite and non-negative".into(),
            ));
        }
        self.command(&format!(
            "seat agent-seat cursor set {:.0} {:.0}",
            point.x, point.y
        ))?;
        Ok(true)
    }

    pub fn click_agent_pointer(&self, button: u8) -> Result<bool, DesktopError> {
        if !(1..=9).contains(&button) {
            return Err(DesktopError::Operation(
                "pointer button must be between 1 and 9".into(),
            ));
        }
        self.command(&format!(
            "seat agent-seat cursor press button{button}; seat agent-seat cursor release button{button}"
        ))?;
        Ok(true)
    }

    pub fn focused(&self, include_sensitive: bool) -> Result<FocusedContext, DesktopError> {
        let window = self.windows()?.into_iter().find(|window| window.focused);
        let workspace = self
            .workspaces()?
            .into_iter()
            .find(|workspace| workspace.focused);
        let working_directory = window
            .as_ref()
            .and_then(|value| value.process_id)
            .and_then(process_cwd);
        let git_root = working_directory
            .as_ref()
            .and_then(|path| find_git_root(path));
        let clipboard_mime_types = command_lines("wl-paste", &["--list-types"]).unwrap_or_default();
        let selected_text = include_sensitive
            .then(|| command_text("wl-paste", &["--primary", "--no-newline"]))
            .flatten()
            .filter(|text| !text.trim().is_empty());
        let browser_url = include_sensitive
            .then(|| browser_url(window.as_ref()))
            .flatten();
        Ok(FocusedContext {
            application: window.as_ref().and_then(|value| value.app_id.clone()),
            process: window
                .as_ref()
                .and_then(|value| value.process_id)
                .map(|value| value.to_string()),
            document_title: window.as_ref().and_then(|value| value.title.clone()),
            working_directory,
            git_root,
            selected_text,
            browser_url,
            clipboard_mime_types,
            workspace: workspace.as_ref().map(|value| value.name.clone()),
            output: workspace.map(|value| value.output),
            ..FocusedContext::default()
        })
    }

    fn query(&self, message_type: &str) -> Result<Value, DesktopError> {
        let output = Command::new(&self.swaymsg)
            .args(["-r", "-t", message_type])
            .output()
            .map_err(|error| DesktopError::Unavailable(format!("Sway IPC: {error}")))?;
        if !output.status.success() {
            return Err(DesktopError::Operation(
                String::from_utf8_lossy(&output.stderr).trim().into(),
            ));
        }
        serde_json::from_slice(&output.stdout)
            .map_err(|error| DesktopError::Operation(error.to_string()))
    }

    fn command(&self, command: &str) -> Result<Value, DesktopError> {
        let output = Command::new(&self.swaymsg)
            .arg("-r")
            .arg(command)
            .output()
            .map_err(|error| DesktopError::Unavailable(format!("Sway IPC: {error}")))?;
        if !output.status.success() {
            return Err(DesktopError::Operation(
                String::from_utf8_lossy(&output.stderr).trim().into(),
            ));
        }
        let value: Value = serde_json::from_slice(&output.stdout)
            .map_err(|error| DesktopError::Operation(error.to_string()))?;
        let succeeded = value
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .all(|item| item.get("success").and_then(Value::as_bool) == Some(true))
            })
            .unwrap_or(false);
        if !succeeded {
            return Err(DesktopError::Operation(value.to_string()));
        }
        Ok(value)
    }
}

fn collect_windows(value: &Value, workspace: Option<&str>, result: &mut Vec<WindowDescriptor>) {
    let kind = value.get("type").and_then(Value::as_str);
    let current_workspace = if kind == Some("workspace") {
        value.get("name").and_then(Value::as_str)
    } else {
        workspace
    };
    let app_id = value
        .get("app_id")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            value
                .pointer("/window_properties/class")
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
    let pid = value
        .get("pid")
        .and_then(Value::as_u64)
        .and_then(|pid| u32::try_from(pid).ok());
    if app_id.is_some() || pid.is_some() {
        result.push(WindowDescriptor {
            id: value.get("id").and_then(Value::as_i64).unwrap_or_default(),
            app_id,
            title: value.get("name").and_then(Value::as_str).map(str::to_owned),
            process_id: pid,
            workspace: current_workspace.map(str::to_owned),
            focused: value
                .get("focused")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            rect: value.get("rect").cloned().unwrap_or(Value::Null),
        });
    }
    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = value.get(key).and_then(Value::as_array) {
            for child in children {
                collect_windows(child, current_workspace, result);
            }
        }
    }
}

fn process_cwd(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
}

fn find_git_root(path: &std::path::Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|candidate| candidate.join(".git").exists())
        .map(std::path::Path::to_owned)
}

fn command_lines(program: &str, args: &[&str]) -> Option<Vec<String>> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    output.status.success().then(|| {
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_owned)
            .collect()
    })
}

fn command_text(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Browser context is opt-in. When a browser is launched with a loopback CDP
/// endpoint, correlate the focused Sway title with its page metadata instead
/// of scraping pixels or browser profile files.
fn browser_url(window: Option<&WindowDescriptor>) -> Option<String> {
    let base = env::var("ARC_BROWSER_CDP").ok()?;
    let endpoint = format!("{}/json", base.trim_end_matches('/'));
    let raw = command_text(
        "curl",
        &[
            "--fail",
            "--silent",
            "--show-error",
            "--max-time",
            "1",
            &endpoint,
        ],
    )?;
    let pages = serde_json::from_str::<Value>(&raw)
        .ok()?
        .as_array()?
        .clone();
    let focused_title = window
        .and_then(|value| value.title.as_deref())
        .unwrap_or_default();
    pages
        .iter()
        .filter(|page| page.get("type").and_then(Value::as_str) == Some("page"))
        .find(|page| {
            let title = page
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default();
            !title.is_empty() && (focused_title.contains(title) || title.contains(focused_title))
        })
        .or_else(|| {
            pages
                .iter()
                .find(|page| page.get("type").and_then(Value::as_str) == Some("page"))
        })
        .and_then(|page| page.get("url"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

pub struct UnsupportedAdapter {
    pub platform: &'static str,
}

#[async_trait]
impl DesktopAdapter for UnsupportedAdapter {
    async fn focused_context(&self) -> Result<FocusedContext, DesktopError> {
        Ok(FocusedContext::default())
    }
    async fn capture_screen(&self) -> Result<Vec<u8>, DesktopError> {
        Err(DesktopError::Unavailable(format!(
            "capture on {}",
            self.platform
        )))
    }
    async fn list_applications(&self) -> Result<Vec<Application>, DesktopError> {
        Ok(vec![])
    }
    async fn launch_application(&self, _: &str) -> Result<(), DesktopError> {
        Err(DesktopError::Unavailable(self.platform.into()))
    }
    async fn move_pointer(&self, _: Point) -> Result<(), DesktopError> {
        Err(DesktopError::Unavailable(self.platform.into()))
    }
    async fn click(&self, _: u8) -> Result<(), DesktopError> {
        Err(DesktopError::Unavailable(self.platform.into()))
    }
    async fn type_text(&self, _: &str) -> Result<(), DesktopError> {
        Err(DesktopError::Unavailable(self.platform.into()))
    }
    async fn press_keys(&self, _: &[String]) -> Result<(), DesktopError> {
        Err(DesktopError::Unavailable(self.platform.into()))
    }
    async fn read_clipboard(&self) -> Result<String, DesktopError> {
        Err(DesktopError::Unavailable(self.platform.into()))
    }
    async fn write_clipboard(&self, _: &str) -> Result<(), DesktopError> {
        Err(DesktopError::Unavailable(self.platform.into()))
    }
    async fn stop_control(&self) -> Result<(), DesktopError> {
        Ok(())
    }
}
