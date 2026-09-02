# Waybar Auto-Hide
 
A lightweight utility that automatically shows and hides Waybar in Hyprland based on cursor position and window state. It hides Waybar when no window is open in the current workspace, and temporarily makes it visible when the cursor is placed at the edge of the screen.
 
## Features
 
- Automatically hides Waybar when a window is open in the current workspace.
- Temporarily shows Waybar when the cursor is placed at its edge.
- Hides Waybar again as soon as the cursor moves away.
- Supports multi-monitor setups, with each bar tracking its own monitor.
- Supports multiple Waybar instances, including several bars on the same monitor (one at the top and one at the bottom, for example).
- Reads each bar's `position` straight from its Waybar config, so every instance uses the correct edge with no extra setup.
- Optional "always hidden" mode, which only shows a bar when the cursor is at its edge.
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

3. ***[REQUIRED] Add the following lines to your waybar config***
   
      ```jsonc
      {
        "on-sigusr1": "hide",
        "on-sigusr2": "show",
         // ...the rest of your config
      }
      ```
             
   Visibility is controlled through **SIGUSR1** and **SIGUSR2**. Waybar's defaults are `toggle` for SIGUSR1 and `reload` for SIGUSR2, which don't work here: Waybar can't report its own state, so `toggle` drifts out of sync, and `reload` restarts the bar on every show, causing flicker and unnecessary I/O.
 
    Waybar Auto-Hide reads your config at startup and checks these values. If they're missing or wrong, it will craash and send a notification.

4. **Add to your Hyprland config** (`~/.config/hypr/hyprland.conf`):
   ```bash
   exec-once = $HOME/.config/hypr/scripts/waybar_auto_hide &
   ```
   ...or with the new lua syntax:
   ```lua
   hl.on("hyprland.start", function()
	   hl.exec_cmd("$HOME/.config/hypr/scripts/waybar_auto_hide &")
   end)
   ```


   Make sure to launch waybar_auto_hide **after** your waybar instances. If you launch waybar from hyprland, you can add a sleep delay before launching it:
   ```bash
   sleep 1 && $HOME/.config/hypr/scripts/waybar_auto_hide &
   ```
   


5. **Restart your Hyprland session** (reloading is not enough, a full reboot is recomended)


## Customization
 
### Always Hidden Mode
 
Bars only appear when you move the cursor to their edge, whether or not windows are open:
 
```bash
waybar_auto_hide --always-hidden
```
 
**Example:**
 
```
exec-once = $HOME/.config/hypr/scripts/waybar_auto_hide --always-hidden &
```
 
### Bar Position
 
There's no option to set this. Each bar's edge is read from the `position` field of its own Waybar config, which is what makes per-instance positions possible — a top bar and a bottom bar on the same monitor each respond to their own edge.
 

## True multi-monitor setups
 
What follows is an example, not part of the project. All that's actually required is **one Waybar process per monitor**, each started with a config naming its own `output`. A config with no `output` spans every monitor, but those bars share one process, so a signal hides all of them at once.
 
Rather than maintaining a near-identical config per monitor, keep one template at `~/.config/waybar/config.jsonc` with a placeholder:
 
```jsonc
{
  "output": "REPLACE_OUTPUT",
  "position": "top",
  "on-sigusr1": "hide",
  "on-sigusr2": "show",
  // ...the rest of your config
}
```
 
Then copy it once per monitor, substituting the name (requires `jq`):
 
```bash
#!/usr/bin/env bash
for m in $(hyprctl monitors -j | jq -r '.[].name'); do
    sed "s/REPLACE_OUTPUT/$m/" ~/.config/waybar/config.jsonc > "/tmp/waybar-$m.jsonc"
    waybar -c "/tmp/waybar-$m.jsonc" &
done
```
 
Save it as `~/.config/hypr/scripts/waybar-launch.sh`, `chmod +x` it, and launch both:
 
```
hl.exec_cmd("$HOME/.config/hypr/scripts/waybar-launch.sh")
hl.exec_cmd("sleep 1 && $HOME/.config/hypr/scripts/waybar_auto_hide &")
```
 
**Order matters.** Waybar Auto-Hide scans for Waybar processes once, at startup, so give the bars a moment to come up.

## Upgrading from an earlier version
 
- **`--side` has been removed.** Position now comes from each Waybar config's `position` field. Passing `--side` prints a warning and is otherwise ignored, so existing launch commands won't break, but you should drop it.
- **The signal config is now required, not just recommended.** Bars without `"on-sigusr1": "hide"` and `"on-sigusr2": "show"` will cause the program to exit with a notification instead of misbehaving silently.

## Special Thanks
- [@raresgoidescu](https://github.com/raresgoidescu) for implementing multi-monitor support and direct Unix socket communication with waybar, improving performance.
- Everyone who provided feedback, reported bugs, and opened issues!

