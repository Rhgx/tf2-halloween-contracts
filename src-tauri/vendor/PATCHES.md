# Vendored steam-vent patches

Base (copied from the steam spike): https://codeberg.org/steam-vent/steam-vent at commit b0e40109dd7a1b5101e1546ed27f62663dbe5c4b (v0.6.0, 2026-06-28). Copied as plain source with `.git` removed.

- Patch A (`src/game_coordinator/mod.rs`): `auth_details` was `todo!()`, which panicked on every `GameCoordinator` send. It now returns `Some(&self.auth)`.
- Patch B (`src/connection/unauthenticated.rs`): adds `pub async fn service_method` on `UnAuthenticatedConnection`, forwarding to the crate's private `service_method_un_authenticated`, so the QR sign-in calls work before login.
- Patch C (`Cargo.toml`, `src/transport/websocket.rs`): TLS uses rustls with the `ring` provider only (`rustls` and `reqwest` without their default aws-lc-rs features, and `ring::default_provider()` installed in the websocket transport and before the server list's HTTPS request in `src/serverlist.rs`, since reqwest without its provider feature panics otherwise), so building needs no cmake or nasm for aws-lc-sys.
