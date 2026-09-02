use std::{fs, path::PathBuf};

use jsonc_parser::ParseOptions;
use serde::Deserialize;

/// No `output` means no specified monitor output. The process will be used for all workspaces
#[derive(Debug, Clone)]
pub struct WaybarProcess {
    pub pid: i32,
    pub output: Vec<String>,
    pub side: Side,
}

impl WaybarProcess {
    /// Whether this bar is present on the given monitor. No specified output means all monitors are covered
    pub fn covers_monitor(&self, monitor: &str) -> bool {
        self.output.is_empty() || self.output.iter().any(|o| o == monitor)
    }
}

/// Waybar's `output` accepts a single name or a list.
#[derive(Deserialize)]
#[serde(untagged)]
enum Output {
    One(String),
    Many(Vec<String>),
}

impl Output {
    fn into_vec(self) -> Vec<String> {
        match self {
            Output::One(name) => vec![name],
            Output::Many(names) => names,
        }
    }
}

#[derive(Deserialize, Default)]
struct WaybarConfig {
    output: Option<Output>,
    position: Side,
    #[serde(rename = "on-sigusr1")]
    on_sigusr1: Option<String>,
    #[serde(rename = "on-sigusr2")]
    on_sigusr2: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum Side {
    #[default]
    Top,
    Left,
    Right,
    Bottom,
}

/// Find all the waybar processes, and tries to read their configs and get their associated monitor output.
pub fn waybar_processes() -> Vec<WaybarProcess> {
    procfs::process::all_processes()
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|p| {
            let comm = p.stat().ok()?.comm;
            match comm.as_str() {
                "waybar" | ".waybar-wrapped" => {}
                _ => return None,
            }
            let argv = p.cmdline().ok()?;
            let config_path = config_path_from_flags(&argv).or_else(default_config_path);
            let config = config_path
                .and_then(|c| read_config(&c))
                .unwrap_or_default();

            // Warn users if they don't have their configs properly set
            verify_waybar_config(p.pid, &config);

            Some(WaybarProcess {
                pid: p.pid,
                output: config.output.map(Output::into_vec).unwrap_or_default(),
                side: config.position,
            })
        })
        .collect()
}

/// Handles `-c PATH`, `--config PATH` and `--config=PATH`.
fn config_path_from_flags(argv: &[String]) -> Option<PathBuf> {
    let mut it = argv.iter();
    while let Some(arg) = it.next() {
        if arg == "-c" || arg == "--config" {
            return it.next().map(PathBuf::from);
        }
        if let Some(v) = arg.strip_prefix("--config=") {
            return Some(PathBuf::from(v));
        }
    }
    None
}

/// Default config path when no -c flag is passed (~/.config/waybar/config etc)
/// Follows Waybar's own search order.
fn default_config_path() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    let config_home = match std::env::var_os("XDG_CONFIG_HOME").filter(|x| !x.is_empty()) {
        Some(x) => PathBuf::from(x),
        None => home.join(".config"),
    };

    let dirs = [
        config_home.join("waybar"),
        home.join(".config/waybar"),
        home.join("waybar"),
        PathBuf::from("/etc/xdg/waybar"),
    ];

    dirs.iter()
        .flat_map(|dir| ["config", "config.jsonc"].map(|name| dir.join(name)))
        .find(|path| path.is_file())
}

/// Reads the `output` field from the given config file
fn read_config(config: &std::path::Path) -> Option<WaybarConfig> {
    let raw = fs::read_to_string(config).ok()?;
    let value = jsonc_parser::parse_to_serde_value(&raw, &ParseOptions::default()).ok()?;
    serde_json::from_value::<WaybarConfig>(value).ok()
}

/// Waybar defaults to `toggle` on SIGUSR1 and `reload` on SIGUSR2.
/// Verifies that the config is set properly
fn verify_waybar_config(pid: i32, config: &WaybarConfig) {
    let hide = config.on_sigusr1.as_deref() == Some("hide");
    let show = config.on_sigusr2.as_deref() == Some("show");

    if hide && show {
        return;
    }

    let msg = format!(
        "waybar_auto_hide: waybar {pid} needs \"on-sigusr1\": \"hide\" and \
         \"on-sigusr2\": \"show\" in its config (currently {}, {})",
        config.on_sigusr1.as_deref().unwrap_or("toggle"),
        config.on_sigusr2.as_deref().unwrap_or("reload"),
    );

    crate::hyprland::notify(0, 15000, &msg); // 0 = warning
    panic!("{msg}");
}
