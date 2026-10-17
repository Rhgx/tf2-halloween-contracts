# Credits

Team Fortress 2 and everything in it belong to Valve Corporation. This is a fan-made tool and is not affiliated with or endorsed by Valve.

## Fonts

`public/fonts/TF2Build.ttf` and `public/fonts/TF2Secondary.ttf` are Valve's TF2 fonts, taken from the Team Fortress 2 DIY Kit that Valve publishes for the community at <https://www.teamfortress.com/artwork.php>.

## Map screenshots

The screenshots in `public/maps/` come from the map pages of the Official Team Fortress Wiki, <https://wiki.teamfortress.com>, resized to 640 pixels wide and stored as lossless WebP (JPEGs are kept as they came). They are screenshots of Valve's game, uploaded by wiki contributors. A few maps that had no wiki screenshot yet use the game's own casual menu thumbnail instead.

## Map data

The map list, casual menu grouping, display names, and ConTracker node ids are read from the game's own files (`items_game.txt`, `tf_english.txt`, `proto_defs.vpd`) by `scripts/extract-game.ts`.

## Libraries

- [steam-vent](https://codeberg.org/steam-vent/steam-vent) by Robin Appelman, MIT. Steam sign-in and the Game Coordinator connection. The app uses a patched copy in `src-tauri/vendor/steam-vent`.
- [tf-demo-parser](https://codeberg.org/demostf/parser), MIT or Apache-2.0. Reads the demo file for the gargoyle radar.
- [tiny-skia](https://github.com/RazrFalcon/tiny-skia) by Yevhenii Reizner, BSD-3-Clause. Draws the radar overlay.
- [fontdue](https://github.com/mooman219/fontdue) by Joe Cumbo, MIT, Apache-2.0 or Zlib. Renders the radar overlay's text.
- [qrcode](https://github.com/kennytm/qrcode-rust), MIT or Apache-2.0. Draws the Steam sign-in QR code.
- Icons are from [Lucide](https://lucide.dev).
