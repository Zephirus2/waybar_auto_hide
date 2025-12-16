# Waybar Auto-Hide

A lightweight Rust utility that automatically shows/hides Waybar in Hyprland based on cursor position and window state.

## Installation

1. **Build the binary:**

   ```
   cargo build --release   
   ```

2. **Copy to your Hyprland config directory:**
   ```bash
   mkdir -p ~/.config/hypr/scripts
   cp target/release/waybar-auto-hide ~/.config/hypr/scripts/
   ```

3. **Add to your Hyprland config** (`~/.config/hypr/hyprland.conf`):
   ```
   exec-once = $HOME/.config/hypr/scripts/waybar-auto-hide &
   ```

4. **Restart your Hyprland session** (reloading is not enough)