use std::fs;

pub fn find_waybar_pids() -> Option<Vec<i32>> {
    let pids: Vec<i32> = fs::read_dir("/proc")
        .into_iter()
        .flatten()
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
        .collect();

    (!pids.is_empty()).then_some(pids)
}
