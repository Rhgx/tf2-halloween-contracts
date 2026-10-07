# Steam sign-in and contract sync

## Sign-in

The app signs in the same way the Steam desktop client does when you scan a QR code: it asks Steam for a QR login challenge, shows the code, and waits for the Steam mobile app to approve it. No username or password is entered in the app, and the app never sees them.

When the sign-in is approved, Steam hands back a refresh token. The app keeps that token in `steam-token.txt` inside its data folder and uses it for every later sync. **Sign out of Steam** in the app's footer deletes the file.

The data folder is `%APPDATA%\tf2-halloween-contracts` when you run the exe, and `data` inside the repo when you run from source (ignored by git, so the token never ends up in a commit).

A refresh token lets its holder log in as you. Treat the data folder like a saved password: do not share it, and sign out before handing it to anyone.

## Sync

A sync logs in to Steam with the saved token, tells Steam it is playing TF2, says hello to the TF2 Game Coordinator, and asks the GC for the account's shared object cache. The cache includes one `CSOQuestMapNode` per ConTracker node. A node whose primary star is earned is a completed contract. The app maps those nodes to maps using the node ids in `src/halloween-maps.json`, saves the result to `contracts.json` in its data folder, and logs off. The whole thing takes a few seconds.

The app only reads from the GC. It never sends anything that changes your account, inventory, or contracts.

## Why sync is blocked while TF2 runs

Steam allows one game session per account. If the app claimed the TF2 session while the game was open, Steam would kick you out of the game with a "logged in elsewhere" message. The app checks Steam's playing state before it does anything, and refuses to sync if your account is already in a game. Once TF2 closes, the app waits for Steam to notice and syncs on its own, retrying a few times over the first half minute.

## Marking contracts by hand

While TF2 is running you can mark a contract done in the app. Manual marks live in `data/manual.json` and count as completed until the next sync. When a sync confirms a contract, its manual mark is dropped, so the file only ever holds marks Steam has not confirmed yet.
