# Credits

Team Fortress 2 and everything in it belong to Valve Corporation. This is a fan-made tool and is not affiliated with or endorsed by Valve.

## Fonts

`public/fonts/TF2Build.ttf` and `public/fonts/TF2Secondary.ttf` are Valve's TF2 fonts, taken from the Team Fortress 2 DIY Kit that Valve publishes for the community at <https://www.teamfortress.com/artwork.php>.

## Map screenshots

The screenshots in `public/maps/` come from the map pages of the Official Team Fortress Wiki, <https://wiki.teamfortress.com>, resized to 640 pixels wide. They are screenshots of Valve's game, uploaded by wiki contributors. A few maps that had no wiki screenshot yet use the game's own casual menu thumbnail instead.

## Map data

The map list, casual menu grouping, display names, and ConTracker node ids are read from the game's own files (`items_game.txt`, `tf_english.txt`, `proto_defs.vpd`) by `scripts/extract-game.ts`.

## Libraries

Steam sign-in and the Game Coordinator connection use [steam-session](https://github.com/DoctorMcKay/node-steam-session) and [steam-user](https://github.com/DoctorMcKay/node-steam-user) by Alex Corn (DoctorMcKay). Icons are from [Lucide](https://lucide.dev).
