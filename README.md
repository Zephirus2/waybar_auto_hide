# Waybar Auto-Hide

A lightweight utility that automatically shows/hides Waybar in Hyprland based on cursor position and window state. It will hide waybar when no window is opened in the current workspace, and will temporarly make it visible when the cursor is placed at the top of the screen. 

## Features
- Automatically hides Waybar when no window is open in the current workspace.
- Temporarily shows Waybar when the cursor is placed at the top of the screen.
- Hides Waybar again as soon as the cursor moves away.
- Supports multi-monitor setups
- Supports all waybar positions (top, bottom...). See the "customization" section at the bottom.
- Optional "always hidden" mode, which only shows the bar when the cursor is on top.
- Works out of the box with no additional dependencies.

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

6. **Restart your Hyprland session** (reloading is not enough, a full reboot is recomended)


## Customization

You can customize the behavior of waybar-auto-hide using command-line options:

### Change Waybar Position
To specify which edge of the screen will be used to show waybar with the mouse, add the following argument:
```bash
waybar_auto_hide --side [top|bottom|left|right]  #default is top
```
### Always Hidden Mode
Enable "always hidden" mode, where Waybar only appears when you move your cursor to its edge (regardless of whether windows are open):
```bash
waybar_auto_hide --always-hidden
```
**Example:**
```bash
exec-once = $HOME/.config/hypr/scripts/waybar_auto_hide --side bottom --always-hidden &
```
## Special Thanks
- [@raresgoidescu](https://github.com/raresgoidescu) for implementing multi-monitor support and direct Unix socket communication with waybar, improving performance.
- Everyone who provided feedback, reported bugs, and opened issues!

