# How the app works with TF2

## The casual queue

TF2 stores the maps you ticked in the casual map selection in `tf/casual_criteria.vdf` inside the game folder. The app writes that file when you change your queue here. TF2 reads it when you click **Restore** in the casual map selection, so you need to click that once after changing the queue.

Before the first write, the app copies your original file to `casual_criteria.backup.json` in its data folder. The original is put back when the app quits, whether through the tray's **Quit**, the **Close app and restore map selection** link, or closing the window with Close to tray off. If the app crashes before it can restore, the backup is restored the next time it starts.

With **Restore original casual map selection on quit** turned off in Settings, the backup is dropped when the app quits and the app's queue stays in TF2. The next queue change takes a new backup.

## Following your matches

TF2 writes its console output to `tf/console.log` when it is launched with `-condebug` in its Steam launch options. The app tails that file and looks for a handful of lines:

- `[PartyClient] Entering queue` and `Leaving queue` mark when you are searching for a match.
- `Lobby created` means a match was found.
- `Map: <name>` names the map being loaded. It repeats on every map change in the same match.
- `Lobby destroyed` means the match ended.

Together with a check of whether TF2 is running (`tf_win64.exe` in the process list), this is enough to show whether you are in queue, loading, or on a map, and to ask about the contract when a Halloween map ends. Contract tracking never attaches to the game or reads its memory. The gargoyle radar is the only part that sends commands to TF2, over RCON (see [the radar doc](radar.md)).

If the log file is not there, Settings shows `console.log not found` and the rest keeps working.

While TF2 is closed, the app trims `tf/console.log` to its newest 1 MB once it passes 10 MB, and deletes `tf/gargoyle_radar.dem`, the demo file the radar records into.

## Launching TF2

**Launch TF2** opens `steam://rungameid/440`, the same thing as pressing Play in Steam. If you are signed in, the app syncs first, so the session starts from Steam's latest contract state.

## Where TF2 is

The app reads Steam's install folder from the registry (`HKCU\Software\Valve\Steam\SteamPath`), then checks every library listed in Steam's `libraryfolders.vdf` for a `Team Fortress 2` folder, and finally the default `C:\Program Files (x86)\Steam` location. The folder in use is shown in Settings.

If it gets the folder wrong, pick the game folder (the one containing `tf`) with **Change** in Settings. Or set the `TF2_DIR` environment variable to that folder and restart the app. `TF2_DIR` takes precedence over the lookup, and an empty value means no TF2.

If TF2 is not found at all, the app still runs: the checklist and Steam sync work, but queueing and match tracking are off until TF2 is found.
