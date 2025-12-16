# Waybar Auto-Hide

A lightweight utility that automatically shows/hides Waybar in Hyprland based on cursor position and window state. It will hide waybar when no window is opened in the current workspace, and will temporarly make it visible when the cursor is placed at the top of the screen. 

## Installation

1. **Build the binary:** 

   ```bash
   git clone https://github.com/Zephirus2/waybar_auto_hide.git
   cd waybar_auto_hide/
   cargo build --release   
   ```
   ...or download a prebuilt binary in [releases](https://github.com/Zephirus2/waybar_auto_hide/releases/download/Release/waybar-auto_hide)
2. **Copy it to your Hyprland config directory:**
   ```bash
   mkdir -p ~/.config/hypr/scripts
   cp target/release/waybar-auto-hide ~/.config/hypr/scripts/
   ```

3. **Add to your Hyprland config** (`~/.config/hypr/hyprland.conf`):
   ```
   exec-once = $HOME/.config/hypr/scripts/waybar-auto-hide &
   ```

4. **Restart your Hyprland session** (reloading is not enough)
