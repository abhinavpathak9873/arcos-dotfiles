use arc_protocol::{AppDescriptor, AppSource, ControlRoute};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct LaunchStats {
    attempts: u64,
    successes: u64,
    last_used_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct RegistryData {
    #[serde(default)]
    stats: BTreeMap<String, LaunchStats>,
    #[serde(default)]
    learned_aliases: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone)]
struct AppRecord {
    descriptor: AppDescriptor,
    desktop_file: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("application not found: {0}")]
    NotFound(String),
    #[error("invalid application alias")]
    InvalidAlias,
    #[error("application launch failed: {0}")]
    Launch(String),
    #[error("application registry storage error: {0}")]
    Storage(#[from] std::io::Error),
    #[error("invalid application registry: {0}")]
    Invalid(#[from] serde_json::Error),
}

pub struct AppRegistry {
    records: BTreeMap<String, AppRecord>,
    desktop_dirs: Vec<PathBuf>,
    path_dirs: Vec<PathBuf>,
    appimage_dirs: Vec<PathBuf>,
    data_path: PathBuf,
    data: RegistryData,
}

impl AppRegistry {
    pub fn open_default(state_dir: &Path) -> Result<Self, AppError> {
        let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let data_home = env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share"));
        let mut desktop_dirs = vec![
            data_home.join("applications"),
            data_home.join("flatpak/exports/share/applications"),
        ];
        let data_dirs =
            env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
        desktop_dirs.extend(
            data_dirs
                .split(':')
                .filter(|item| !item.is_empty())
                .map(|item| PathBuf::from(item).join("applications")),
        );
        desktop_dirs.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));
        let path_dirs = env::split_paths(&env::var_os("PATH").unwrap_or_default()).collect();
        let appimage_dirs = vec![home.join("Applications"), home.join("Downloads")];
        Self::open(
            desktop_dirs,
            path_dirs,
            appimage_dirs,
            state_dir.join("app-registry.json"),
        )
    }

    pub fn open(
        desktop_dirs: Vec<PathBuf>,
        path_dirs: Vec<PathBuf>,
        appimage_dirs: Vec<PathBuf>,
        data_path: PathBuf,
    ) -> Result<Self, AppError> {
        let data = if data_path.exists() {
            serde_json::from_slice(&fs::read(&data_path)?)?
        } else {
            RegistryData::default()
        };
        let mut registry = Self {
            records: BTreeMap::new(),
            desktop_dirs,
            path_dirs,
            appimage_dirs,
            data_path,
            data,
        };
        registry.refresh()?;
        Ok(registry)
    }

    pub fn refresh(&mut self) -> Result<usize, AppError> {
        let mut records = BTreeMap::new();
        for directory in &self.desktop_dirs {
            let mut files: Vec<_> = match fs::read_dir(directory) {
                Ok(entries) => entries
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .collect(),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            files.sort();
            for path in files {
                if path.extension().and_then(|value| value.to_str()) != Some("desktop") {
                    continue;
                }
                if let Some(record) = parse_desktop_entry(&path)? {
                    records
                        .entry(record.descriptor.id.clone())
                        .or_insert(record);
                }
            }
        }
        for directory in &self.appimage_dirs {
            let entries = match fs::read_dir(directory) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                let is_appimage = path
                    .extension()
                    .and_then(|value| value.to_str())
                    .is_some_and(|value| value.eq_ignore_ascii_case("appimage"));
                if !is_appimage {
                    continue;
                }
                let name = path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or("AppImage")
                    .replace(['-', '_'], " ");
                let id = format!("appimage:{}", slug(&name));
                records.entry(id.clone()).or_insert(AppRecord {
                    descriptor: AppDescriptor {
                        id,
                        name,
                        executable: path.to_string_lossy().into(),
                        aliases: vec![],
                        icon: None,
                        mime_types: vec![],
                        control_routes: vec![ControlRoute::Accessibility, ControlRoute::Visual],
                        source: AppSource::AppImage,
                        version: None,
                        last_used_at: None,
                        launch_success_rate: None,
                    },
                    desktop_file: None,
                });
            }
        }
        // PATH entries are useful for spoken commands such as "open code" even
        // when a package did not install a desktop file. They stay out of the
        // unfiltered app catalog to avoid burying graphical apps in CLI tools.
        for directory in &self.path_dirs {
            let entries = match fs::read_dir(directory) {
                Ok(entries) => entries,
                Err(_) => continue,
            };
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                    continue;
                };
                let id = format!("path:{name}");
                records.entry(id.clone()).or_insert(AppRecord {
                    descriptor: AppDescriptor {
                        id,
                        name: name.into(),
                        executable: path.to_string_lossy().into(),
                        aliases: vec![],
                        icon: None,
                        mime_types: vec![],
                        control_routes: vec![],
                        source: AppSource::Path,
                        version: None,
                        last_used_at: None,
                        launch_success_rate: None,
                    },
                    desktop_file: None,
                });
            }
        }
        for (id, record) in &mut records {
            if let Some(aliases) = self.data.learned_aliases.get(id) {
                record.descriptor.aliases.extend(aliases.iter().cloned());
            }
            record.descriptor.aliases.sort();
            record.descriptor.aliases.dedup();
            if let Some(stats) = self.data.stats.get(id) {
                record.descriptor.last_used_at = stats.last_used_at.clone();
                record.descriptor.launch_success_rate =
                    (stats.attempts > 0).then_some(stats.successes as f32 / stats.attempts as f32);
            }
        }
        self.records = records;
        Ok(self.records.len())
    }

    pub fn query(&self, query: &str, limit: usize) -> Vec<AppDescriptor> {
        let needle = query.trim().to_lowercase();
        let mut matches: Vec<(u16, &AppDescriptor)> = self
            .records
            .values()
            .filter(|record| !needle.is_empty() || record.descriptor.source != AppSource::Path)
            .filter_map(|record| {
                let score = app_score(&record.descriptor, &needle);
                (needle.is_empty() || score > 0).then_some((score, &record.descriptor))
            })
            .collect();
        matches.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| source_rank(&left.1.source).cmp(&source_rank(&right.1.source)))
                .then_with(|| left.1.name.to_lowercase().cmp(&right.1.name.to_lowercase()))
        });
        matches
            .into_iter()
            .take(limit.clamp(1, 500))
            .map(|(_, app)| app.clone())
            .collect()
    }

    pub fn get(&self, id: &str) -> Result<&AppDescriptor, AppError> {
        self.records
            .get(id)
            .map(|record| &record.descriptor)
            .ok_or_else(|| AppError::NotFound(id.into()))
    }

    pub fn add_alias(&mut self, id: &str, alias: &str) -> Result<(), AppError> {
        if !self.records.contains_key(id) {
            return Err(AppError::NotFound(id.into()));
        }
        let alias = alias.trim().to_lowercase();
        if alias.len() < 2 || alias.len() > 80 {
            return Err(AppError::InvalidAlias);
        }
        let aliases = self.data.learned_aliases.entry(id.into()).or_default();
        if !aliases.contains(&alias) {
            aliases.push(alias.clone());
        }
        if let Some(record) = self.records.get_mut(id) {
            if !record.descriptor.aliases.contains(&alias) {
                record.descriptor.aliases.push(alias);
            }
        }
        self.persist()
    }

    pub fn launch(&mut self, id: &str) -> Result<(), AppError> {
        let record = self
            .records
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(id.into()))?;
        let stats = self.data.stats.entry(id.into()).or_default();
        stats.attempts += 1;
        stats.last_used_at = Some(Utc::now().to_rfc3339());
        let result = match record.descriptor.source {
            AppSource::DesktopEntry | AppSource::Flatpak | AppSource::Steam => {
                Command::new("gtk-launch").arg(id).spawn().or_else(|_| {
                    let desktop_file = record.desktop_file.as_ref().ok_or_else(|| {
                        std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            "desktop entry path unavailable",
                        )
                    })?;
                    Command::new("gio").arg("launch").arg(desktop_file).spawn()
                })
            }
            AppSource::AppImage | AppSource::Path | AppSource::Learned => {
                Command::new(&record.descriptor.executable).spawn()
            }
        };
        match result {
            Ok(_) => stats.successes += 1,
            Err(error) => {
                self.persist()?;
                return Err(AppError::Launch(error.to_string()));
            }
        }
        if let Some(current) = self.records.get_mut(id) {
            current.descriptor.last_used_at = stats.last_used_at.clone();
            current.descriptor.launch_success_rate =
                Some(stats.successes as f32 / stats.attempts as f32);
        }
        self.persist()
    }

    fn persist(&self) -> Result<(), AppError> {
        if let Some(parent) = self.data_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = self.data_path.with_extension("tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(&self.data)?)?;
        fs::rename(temporary, &self.data_path)?;
        Ok(())
    }
}

fn parse_desktop_entry(path: &Path) -> Result<Option<AppRecord>, AppError> {
    let body = fs::read_to_string(path)?;
    let mut in_desktop = false;
    let mut fields = BTreeMap::new();
    for raw in body.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_desktop = line == "[Desktop Entry]";
            continue;
        }
        if !in_desktop || line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            fields
                .entry(key.trim().to_owned())
                .or_insert(value.trim().to_owned());
        }
    }
    if fields
        .get("Type")
        .is_some_and(|value| value != "Application")
        || is_true(fields.get("Hidden"))
        || is_true(fields.get("NoDisplay"))
    {
        return Ok(None);
    }
    let Some(name) = fields
        .get("Name")
        .filter(|value| !value.is_empty())
        .cloned()
    else {
        return Ok(None);
    };
    let Some(exec) = fields
        .get("Exec")
        .filter(|value| !value.is_empty())
        .cloned()
    else {
        return Ok(None);
    };
    let id = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(&name)
        .to_owned();
    let executable = first_exec_token(&exec).unwrap_or(exec);
    let flatpak = fields.contains_key("X-Flatpak") || path.to_string_lossy().contains("flatpak");
    let steam = id.starts_with("steam-") || executable.ends_with("steam");
    let source = if flatpak {
        AppSource::Flatpak
    } else if steam {
        AppSource::Steam
    } else {
        AppSource::DesktopEntry
    };
    let aliases = builtin_aliases(&id, &name, &executable);
    let control_routes = control_routes(&id, &name, &executable);
    Ok(Some(AppRecord {
        descriptor: AppDescriptor {
            id,
            name,
            executable,
            aliases,
            icon: fields.get("Icon").cloned(),
            mime_types: fields
                .get("MimeType")
                .map(|value| {
                    value
                        .split(';')
                        .filter(|item| !item.is_empty())
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
            control_routes,
            source,
            version: fields.get("X-AppImage-Version").cloned(),
            last_used_at: None,
            launch_success_rate: None,
        },
        desktop_file: Some(path.to_owned()),
    }))
}

fn is_true(value: Option<&String>) -> bool {
    value.is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

fn first_exec_token(exec: &str) -> Option<String> {
    let mut token = String::new();
    let mut quote = None;
    let mut escaped = false;
    for character in exec.trim().chars() {
        if escaped {
            token.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if quote == Some(character) {
            quote = None;
        } else if quote.is_none() && matches!(character, '\'' | '"') {
            quote = Some(character);
        } else if quote.is_none() && character.is_whitespace() {
            break;
        } else {
            token.push(character);
        }
    }
    (!token.is_empty()).then_some(token)
}

fn builtin_aliases(id: &str, name: &str, executable: &str) -> Vec<String> {
    let haystack = format!("{id} {name} {executable}").to_lowercase();
    let mut aliases = BTreeSet::new();
    aliases.insert(slug(name));
    if haystack.contains("chrome") || haystack.contains("chromium") {
        aliases.extend(["browser".into(), "chrome".into(), "web browser".into()]);
    }
    if haystack.contains("dolphin") || haystack.contains("nautilus") {
        aliases.extend(["files".into(), "file manager".into()]);
    }
    if haystack.contains("spotify") {
        aliases.extend(["music".into(), "spotify".into()]);
    }
    if haystack.contains("code") || haystack.contains("codium") {
        aliases.extend(["code editor".into(), "vscode".into()]);
    }
    aliases
        .into_iter()
        .filter(|value| !value.is_empty())
        .collect()
}

fn control_routes(id: &str, name: &str, executable: &str) -> Vec<ControlRoute> {
    let haystack = format!("{id} {name} {executable}").to_lowercase();
    let mut routes = BTreeSet::new();
    if haystack.contains("spotify") || haystack.contains("vlc") || haystack.contains("player") {
        routes.insert(ControlRoute::Mpris);
        routes.insert(ControlRoute::Dbus);
    }
    if haystack.contains("chrome") || haystack.contains("chromium") || haystack.contains("firefox")
    {
        routes.insert(ControlRoute::BrowserCdp);
    }
    routes.insert(ControlRoute::Accessibility);
    routes.insert(ControlRoute::Visual);
    routes.into_iter().collect()
}

fn app_score(app: &AppDescriptor, needle: &str) -> u16 {
    if needle.is_empty() {
        return 1;
    }
    let name = app.name.to_lowercase();
    let id = app.id.to_lowercase();
    if name == needle || id == needle {
        1000
    } else if app.aliases.iter().any(|alias| alias == needle) {
        950
    } else if name.starts_with(needle) {
        850
    } else if app.aliases.iter().any(|alias| alias.starts_with(needle)) {
        800
    } else if name.contains(needle) || id.contains(needle) {
        650
    } else if app.aliases.iter().any(|alias| alias.contains(needle)) {
        600
    } else if needle
        .split_whitespace()
        .all(|word| name.contains(word) || id.contains(word))
    {
        400
    } else {
        0
    }
}

fn source_rank(source: &AppSource) -> u8 {
    match source {
        AppSource::DesktopEntry => 0,
        AppSource::Flatpak => 1,
        AppSource::AppImage => 2,
        AppSource::Steam => 3,
        AppSource::Learned => 4,
        AppSource::Path => 5,
    }
}

fn slug(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn desktop(path: &Path, id: &str, body: &str) {
        fs::create_dir_all(path).unwrap();
        fs::write(path.join(format!("{id}.desktop")), body).unwrap();
    }

    #[test]
    fn discovers_and_semantically_ranks_desktop_apps() {
        let directory = tempfile::tempdir().unwrap();
        let apps = directory.path().join("applications");
        desktop(
            &apps,
            "google-chrome",
            "[Desktop Entry]\nType=Application\nName=Google Chrome\nExec=/usr/bin/google-chrome %U\nIcon=google-chrome\nMimeType=text/html;x-scheme-handler/https;\n",
        );
        desktop(
            &apps,
            "org.kde.dolphin",
            "[Desktop Entry]\nType=Application\nName=Dolphin\nExec=dolphin %U\n",
        );
        desktop(
            &apps,
            "hidden",
            "[Desktop Entry]\nType=Application\nName=Hidden\nExec=hidden\nNoDisplay=true\n",
        );
        let registry = AppRegistry::open(
            vec![apps],
            vec![],
            vec![],
            directory.path().join("registry.json"),
        )
        .unwrap();
        assert_eq!(registry.query("browser", 10)[0].id, "google-chrome");
        assert_eq!(registry.query("files", 10)[0].id, "org.kde.dolphin");
        assert!(registry.query("hidden", 10).is_empty());
    }

    #[test]
    fn learned_aliases_survive_restart() {
        let directory = tempfile::tempdir().unwrap();
        let apps = directory.path().join("applications");
        let data = directory.path().join("registry.json");
        desktop(
            &apps,
            "editor",
            "[Desktop Entry]\nType=Application\nName=Text Editor\nExec=editor\n",
        );
        let mut registry =
            AppRegistry::open(vec![apps.clone()], vec![], vec![], data.clone()).unwrap();
        registry.add_alias("editor", "notes app").unwrap();
        let restored = AppRegistry::open(vec![apps], vec![], vec![], data).unwrap();
        assert_eq!(restored.query("notes app", 1)[0].id, "editor");
    }

    #[test]
    fn parses_quoted_executables_without_desktop_placeholders() {
        assert_eq!(
            first_exec_token("\"/opt/My App/app\" --new %U").as_deref(),
            Some("/opt/My App/app")
        );
    }
}
