use std::{fs, path::PathBuf};

use serde::Deserialize;

/// No `output` means no specified monitor output. The process will be used for all workspaces
#[derive(Debug, Clone)]
pub struct WaybarProcess {
    pub pid: i32,
    pub output: Option<String>,
    pub side: Side,
}

#[derive(Deserialize, Default)]
struct WaybarConfig {
    output: Option<String>,
    position: Side,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Top,
    Left,
    Right,
    Bottom,
}

impl Default for Side {
    fn default() -> Self {
        Side::Top
    }
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

            Some(WaybarProcess {
                pid: p.pid,
                output: config.output,
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

/// Default config path when no -c flag is passed (~/.config/waybar/config)
fn default_config_path() -> Option<PathBuf> {
    let dir = match std::env::var("XDG_CONFIG_HOME") {
        Ok(x) if !x.is_empty() => PathBuf::from(x),
        _ => PathBuf::from(std::env::var("HOME").ok()?).join(".config"),
    }
    .join("waybar");

    ["config", "config.json"]
        .iter()
        .map(|name| dir.join(name))
        .find(|c| c.is_file())
}

/// Reads the `output` field from the given config file
fn read_config(config: &std::path::Path) -> Option<WaybarConfig> {
    let raw = fs::read_to_string(config).ok()?;
    serde_json::from_str::<WaybarConfig>(&raw).ok()
}
