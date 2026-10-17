# Gargoyle radar

The radar draws a panel over TF2 that points to your Halloween soul gargoyle. It has a compass ring with an arrow for the direction, the distance, and a height pin that shows whether the gargoyle is above or below you. Within 2.5 m it shows **HERE**. The panel is a click-through overlay, so clicks pass through to the game.

The radar is Windows only, and TF2 has to run in **Windowed (no border)** mode.

## How it works

- Your position: the app sends `getpos` to TF2 over RCON once per screen refresh. RCON is TF2's own remote console. The launch options turn it on for this PC only.
- The gargoyle: the app runs `record gargoyle_radar`, so TF2 writes a demo file to `tf\gargoyle_radar.dem`, and the app reads it as it grows.
- Demo delay: TF2 writes demos in bursts of about 700 KB, which on a quiet server is about once a minute. The app sends empty `alias` commands over RCON, which TF2 records into the demo, so the bursts come sooner. After each burst the app adjusts how many it sends, so a burst lands every N seconds. N is the **Demo delay** setting, 2 to 20 seconds, default 5. Demo delay is the longest wait before a new gargoyle reaches the demo file. Lower values mean more filler and a faster growing demo file: at 5 s on a quiet server the file can reach about 500 MB an hour.
- No game files are changed and TF2's memory is not read.
- When TF2 closes, the app deletes `tf\gargoyle_radar.dem`.

## Setup

1. Open the **Radar** tab. It shows a launch options line with the RCON port and a generated password:

   ```
   -usercon +ip 0.0.0.0 +hostport <port> +net_start +rcon_password <generated> +sv_rcon_whitelist_address 127.0.0.1 -condebug
   ```

2. In Steam, right-click the game, open Properties, and paste the line into Launch Options.
3. Restart TF2.

The setup steps fold away once the app connects to RCON. The line contains the RCON password, so do not post it. `-condebug` is also what the Contracts tab needs for match tracking, so the same line covers both.

If you change the **RCON port**, the launch options line changes too, so paste the new line into Steam and restart TF2.

## The Radar tab

- **Switch**: turns the radar on and off.
- **Status**: whether TF2 is running, whether RCON is connected, the position rate, the demo interval and filler, and whether the gargoyle is found.
- **Last events**: the latest messages from the radar.
- **Overlay preview**: the panel as it will look in the game.
- **Opacity** and **Scale**: how the panel looks.
- **Move radar**: opens edit mode in the game (see below).
- **Reset layout**: puts the panel back to its default position, scale and opacity.
- **RCON port** and **Demo delay**: see above.
- **Follow any gargoyle**: follows any gargoyle, not only yours. This is for testing: on a local server, run `sv_cheats 1`, then `ent_create tf_halloween_gift_pickup` to create one.

The radar's settings are saved in `radar.json` in the data folder.

## Edit mode

**Move radar** opens edit mode in the game. The panel tells you to press Esc in TF2 to free the mouse.

- Drag the panel to move it. Drag the corner to resize it.
- The arrow keys nudge the panel by 1 px. Hold Shift for 10 px.
- `+` and `-` resize the panel. PgUp and PgDn change the opacity. Home resets the layout.
- The edit hotkey saves and leaves edit mode.

## Hotkeys

- **Pause** (default): turns the overlay on and off.
- **Scroll Lock** (default): starts edit mode, and saves it when pressed again.

To change a hotkey, open **Settings**, click the field under Radar hotkeys, then press a key or Mouse 4 or Mouse 5. Esc cancels. If no key is pressed within 10 seconds, the change is dropped. The keys that edit mode uses are refused.

## Troubleshooting

- **Cannot reach TF2's RCON: add the launch options, then restart TF2.** The launch options are missing, or TF2 was not restarted after adding them.
- **TF2 rejected the RCON password: check the launch options.** The line in Steam does not match the one in the Radar tab. Copy the line again.
- **Launch options not applied yet.** Paste the line into Steam and restart TF2.

RCON tools have been banned when they automated visible actions; the radar only sends `getpos`, `record` and `alias` commands, all to TF2 on this PC.
