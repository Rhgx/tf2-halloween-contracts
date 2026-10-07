# How the app works with TF2

## The casual queue

TF2 stores the maps you ticked in the casual map selection in `tf/casual_criteria.vdf` inside the game folder. The app writes that file when you change your queue here. TF2 reads it when you click **Restore** in the casual map selection, so you need to click that once after changing the queue.

Before the first write, the app copies your original file to `data/casual_criteria.backup.json`. The original is put back when the app closes, whether through the **Close app** link, Ctrl+C, or closing the console window. If the app crashes before it can restore, the backup is restored the next time it starts.

## Following your matches

TF2 writes its console output to `tf/console.log` when it is launched with `-condebug` in its Steam launch options. The app tails that file and looks for a handful of lines:

- `[PartyClient] Entering queue` and `Leaving queue` mark when you are searching for a match.
- `Lobby created` means a match was found.
- `Map: <name>` names the map being loaded. It repeats on every map change in the same match.
- `Lobby destroyed` means the match ended.

Together with a check of whether `tf_win64.exe` is running, this is enough to show whether you are in queue, loading, or on a map, and to ask about the contract when a Halloween map ends. The app never attaches to the game or reads its memory, and it never sends console commands.

If the log file is not there, the app says so under the queue controls and the rest keeps working.

## Launching TF2

**Launch TF2** opens `steam://rungameid/440`, the same thing as pressing Play in Steam. If you are signed in, the app syncs first, so the session starts from Steam's latest contract state.

## Where TF2 is

The app reads Steam's install folder from the registry (`HKCU\Software\Valve\Steam\SteamPath`), then checks every library listed in Steam's `libraryfolders.vdf` for a `Team Fortress 2` folder, and finally the default `C:\Program Files (x86)\Steam` location. The folder it picked is printed in the console at startup. If it gets it wrong, set the `TF2_DIR` environment variable to the game folder (the one containing `tf`) and restart.

If TF2 is not found at all, the app still runs: the checklist and Steam sync work, but queueing and match tracking are off and the page says so.
