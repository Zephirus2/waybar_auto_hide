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
   cp target/release/waybar_auto_hide ~/.config/hypr/scripts/
   ```

3. **Add to your Hyprland config** (`~/.config/hypr/hyprland.conf`):
   ```
   exec-once = $HOME/.config/hypr/scripts/waybar_auto_hide &
   ```
4. ***[RECOMENDED] Add the following lines to your waybar config***
   

   The utility uses **SIGUSR1** and **SIGUSR2** to control visibility. By default, **SIGUSR1** toggles visibility, and **SIGUSR2** reloads the config (making the bar visible). Since Waybar can’t report its state, SIGUSR2 is the only way to ensure positive visibility    and prevent desync, though it may cause slight flicker, delay, or unnecessary I/O.

   It’s recommended to add the following lines to your Waybar config for smoother operation:
      ```
      "on-sigusr1": "hide",
      "on-sigusr2": "show",
      ```
   

   

6. **Restart your Hyprland session** (reloading is not enough)
