use clap::{Parser, ValueEnum};
use serde::Deserialize;
use evdev::{enumerate, Key};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixStream,
    sync::mpsc::{self, Sender},
    thread,
    time::Duration,
};

// The distance from the top at which the bar will activate
const PIXEL_THRESHOLD: i32 = 3;

// The distance from the top at which the bar will hide again.
const PIXEL_THRESHOLD_SECONDARY: i32 = 50;
const MOUSE_REFRESH_DELAY_MS: u64 = 100;

fn main() {
    let args = Args::parse();
    let (tx, rx) = mpsc::channel::<Event>();

    let mut cursor_top: bool = false;
    let mut windows_opened: bool = check_windows();
    let mut super_pressed: bool = false;
    let mut last_visibility: bool = !windows_opened;

    spawn_mouse_position_updater(tx.clone(), args.clone());
    spawn_window_event_listener(tx.clone());
    spawn_super_key_listener(tx.clone());

    tx.send(Event::CursorTop(false)).ok();
    tx.send(Event::WindowsOpened(windows_opened)).ok();

    // Cache Waybar PID to avoid repeated lookups
    let mut waybar_pid = find_waybar_pid();

    for event in rx {
        match event {
            Event::CursorTop(val) => cursor_top = val,
            Event::WindowsOpened(val) => windows_opened = val,
            Event::SuperPressed(val) => super_pressed = val,
        }

        let trigger = cursor_top || (args.enable_super && super_pressed);
        let current_visible = match args.always_hidden {
            true => trigger,
            false => {
                if trigger {
                    true
                } else {
                    !windows_opened
                }
            }
        };

        if current_visible != last_visibility {
            // Refreshes PID if it was lost or not found yet
            if waybar_pid.is_none() {
                waybar_pid = find_waybar_pid();
            }

            if let Some(pid) = waybar_pid {
                if !set_waybar_visible(pid, current_visible) {
                    // If signal fails, Waybar might have restarted
                    waybar_pid = find_waybar_pid();
                    if let Some(new_pid) = waybar_pid {
                        set_waybar_visible(new_pid, current_visible);
                    }
                }
            }
        }
        last_visibility = current_visible
    }
}

fn find_keyboard_device() -> Option<String> {

    let mut fallback: Option<String> = None;

    for (path, device) in enumerate() {
        let name = device.name().unwrap_or("").to_lowercase();

        if let Some(keys) = device.supported_keys() {
            let has_super =
                keys.contains(Key::KEY_LEFTMETA) || keys.contains(Key::KEY_RIGHTMETA);

            if has_super {
                let path_str = path.to_string_lossy().to_string();

                // Prefer keyd virtual keyboard
                if name.contains("keyd") {
                    println!("Using keyd device: {}", path_str);
                    return Some(path_str);
                }

                // fallback to first valid keyboard
                if fallback.is_none() {
                    fallback = Some(path_str);
                }
            }
        }
    }

    if let Some(ref p) = fallback {
        println!("Using fallback keyboard: {}", p);
    }

    fallback
}

/// unix socket listener for super key
fn spawn_super_key_listener(tx: Sender<Event>) {
    use evdev::{Device, InputEventKind, Key};
    use std::time::Duration;

    let path = find_keyboard_device().expect("No keyboard device found");

    thread::spawn(move || {
        loop {
            match Device::open(&path) {
                Ok(mut device) => {
                    let mut last_state = false;
                    loop {
                        match device.fetch_events() {
                            Ok(events) => {
                                for event in events {
                                    if let InputEventKind::Key(key) = event.kind() {
                                        if key == Key::KEY_LEFTMETA {
                                            let new_state = event.value() != 0;

                                            if new_state != last_state {
                                                tx.send(Event::SuperPressed(new_state)).ok();
                                                last_state = new_state;
                                            }
                                        }
                                    }
                                }
                            }
                            Err(_) => {
                                // Device likely disconnected → break and reopen
                                break;
                            }
                        }
                    }
                }
                Err(_) => {
                    // Retry if device not available
                    thread::sleep(Duration::from_secs(1));
                }
            }
        }
    });
}

/// Keeps track of the mouse position
fn spawn_mouse_position_updater(tx: Sender<Event>, args: Args) {
    thread::spawn(move || {
        let mut previous_state = false;
        loop {
            if let (Some(pos), Some(monitors)) = (get_cursor_pos(), get_monitors()) {
                
                // Multi-monitor fix: Find which monitor the cursor is currently on
                // Scaling fix: Mouse position in impl Monitor
                let active_monitor = monitors.iter().find(|m| m.contains(&pos));

                // Scaling fix: Scaling calculation in impl Monitor
                if let Some(m) = active_monitor {
                    let distance_from_edge = match args.side {
                        Side::Top => pos.y - m.y,
                        Side::Bottom => (m.y + m.logical_height()) - pos.y,
                        Side::Left => pos.x - m.x,
                        Side::Right => (m.x + m.logical_width()) - pos.x,
                    };

                    let threshold = if previous_state {
                        PIXEL_THRESHOLD_SECONDARY
                    } else {
                        PIXEL_THRESHOLD
                    };
                    
                    let is_cursor_active = distance_from_edge <= threshold;

                    if is_cursor_active != previous_state {
                        tx.send(Event::CursorTop(is_cursor_active)).ok();
                    }
                    previous_state = is_cursor_active;
                }
            }
            thread::sleep(Duration::from_millis(MOUSE_REFRESH_DELAY_MS));
        }
    });
}

#[derive(Debug)]
enum Event {
    CursorTop(bool),
    WindowsOpened(bool),
    SuperPressed(bool),
}

/// Helper to communicate with Hyprland Socket instead of spawning processes
fn hypr_query(cmd: &str) -> Option<String> {
    let socket_path = format!(
        "{}/hypr/{}/.socket.sock",
        std::env::var("XDG_RUNTIME_DIR").ok()?,
        std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?
    );
    let mut stream = UnixStream::connect(socket_path).ok()?;
    stream.write_all(cmd.as_bytes()).ok()?;
    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    Some(response)
}

fn get_cursor_pos() -> Option<CursorPos> {
    serde_json::from_str(&hypr_query("j/cursorpos")?).ok()
}

fn get_monitors() -> Option<Vec<Monitor>> {
    serde_json::from_str(&hypr_query("j/monitors")?).ok()
}

fn spawn_window_event_listener(tx: mpsc::Sender<Event>) {
    thread::spawn(move || {
        let socket_path = format!(
            "{}/hypr/{}/.socket2.sock",
            std::env::var("XDG_RUNTIME_DIR").unwrap(),
            std::env::var("HYPRLAND_INSTANCE_SIGNATURE").unwrap()
        );

        let stream = match UnixStream::connect(&socket_path) {
            Ok(s) => s,
            Err(_) => return,
        };

        let reader = BufReader::new(stream);
        for line in reader.lines().flatten() {
            if line.contains("window") || line.contains("workspace") {
                tx.send(Event::WindowsOpened(check_windows())).ok();
            }
        }
    });
}

fn check_windows() -> bool {
    let res = hypr_query("j/activeworkspace").unwrap_or_default();
    let data: serde_json::Value = serde_json::from_str(&res).unwrap_or_default();
    data["windows"].as_i64().unwrap_or(0) > 0
}

/// Uses direct syscalls to signal Waybar
fn set_waybar_visible(pid: i32, visible: bool) -> bool {
    let signal = if visible { 12 } else { 10 }; // SIGUSR2 (show), SIGUSR1 (hide)
    unsafe { libc::kill(pid, signal) == 0 }
}

fn find_waybar_pid() -> Option<i32> {
    fs::read_dir("/proc")
        .ok()?
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if !path.is_dir() {
                return None;
            }
            let comm = fs::read_to_string(path.join("comm")).ok()?;
            if comm.trim() == "waybar" || comm.trim() == ".waybar-wrapped" {
                path.file_name()?.to_str()?.parse::<i32>().ok()
            } else {
                None
            }
        })
        .next()
}

#[derive(Deserialize)]
struct CursorPos {
    x: i32,
    y: i32,
}

#[derive(Deserialize)]
struct Monitor {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    scale: f64, // Scaling fix: Get scaling factor
}

// Scaling fix: impl to fix scaling and check for cursor out of main loop

impl Monitor {
    fn logical_width(&self) -> i32 {
        (self.width as f64 / self.scale) as i32
    }

    fn logical_height(&self) -> i32 {
        (self.height as f64 / self.scale) as i32
    }

    fn contains(&self, pos: &CursorPos) -> bool {
        pos.x >= self.x
            && pos.x < self.x + self.logical_width()
            && pos.y >= self.y
            && pos.y < self.y + self.logical_height()
    }
}

#[derive(Parser, Clone)]
struct Args {
    /// If set, the bar will always hide when the cursor is not at the top
    #[arg(long)]
    always_hidden: bool,
    #[arg(long)]
    enable_super: bool,
    /// Which side of the screen the bar is located on. Beware that doesn't work well with multiple monitors.
    #[arg(long, value_enum, default_value = "top")]
    side: Side,
}

#[derive(Debug, Clone, Copy, Deserialize, ValueEnum)]
enum Side {
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
