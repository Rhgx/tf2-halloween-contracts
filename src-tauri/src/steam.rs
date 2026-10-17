// Everything that talks to Steam: the one-time QR sign-in, and reading completed contracts from the TF2 Game
// Coordinator with the refresh token it produced. Read only: nothing here changes the account.

use std::collections::BTreeSet;
use std::future::Future;
use std::io::Cursor;
use std::path::PathBuf;
use std::pin::{pin, Pin};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::future::{FutureExt, Shared};
use futures_util::StreamExt;
use qrcode::render::svg;
use qrcode::QrCode;
use serde::Serialize;
use steam_vent::auth::{Os, RefreshToken, RefreshTokenError};
use steam_vent::connection::UnAuthenticatedConnection;
use steam_vent::{
    Connection, ConnectionError, ConnectionTrait, EResult, GameCoordinator, LoginError,
    NetworkError, ServerList,
};
use steam_vent_proto::enums_clientserver::EMsg;
use steam_vent_proto::protobuf::{EnumOrUnknown, MessageField};
use steam_vent_proto::steammessages_auth_steamclient::{
    CAuthentication_BeginAuthSessionViaQR_Request, CAuthentication_DeviceDetails,
    CAuthentication_PollAuthSessionStatus_Request, EAuthTokenPlatformType,
};
use steam_vent_proto::steammessages_clientserver_2::CMsgClientPlayingSessionState;
use steam_vent_proto::tf2::gcsdk_gcmessages::{
    CMsgSOCacheSubscribed, CMsgSOCacheSubscriptionCheck, CMsgSOCacheSubscriptionRefresh,
};
use steam_vent_proto::tf2::tf_gcmessages::CSOQuestMapNode;
use steam_vent_proto::{MsgKind, RpcMessage};
use tauri::async_runtime::JoinHandle;
use tokio::time::{sleep, timeout};

use crate::maps::MAPS;
use crate::store::{self, Contracts};
use crate::tf2::process;

const APP_ID: u32 = 440;
const DEVICE_NAME: &str = "TF2 Halloween Utils";
const LOGIN_TIMEOUT: Duration = Duration::from_secs(120);
const PLAYING_STATE_WAIT: Duration = Duration::from_secs(10);
const SYNC_TIMEOUT: Duration = Duration::from_secs(45);
const QUEST_MAP_NODE_TYPE: i32 = 44;
const SIGN_IN_EXPIRED: &str = "Your Steam sign-in expired. Sign in again to sync.";

#[derive(Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LoginStatus {
    Waiting,
    Scanned,
    Failed,
}

/// The QR sign-in in progress, as the sign-in dialog shows it.
#[derive(Clone, Serialize)]
pub struct LoginState {
    pub status: LoginStatus,
    /// The QR code as an SVG data URL.
    pub qr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

type SyncRun = Shared<Pin<Box<dyn Future<Output = Result<(), String>> + Send>>>;

/// Managed Tauri state: the sign-in in progress and the sync in progress (shared by everyone who asks for one).
pub struct Steam {
    token_file: PathBuf,
    contracts_file: PathBuf,
    manual_file: PathBuf,
    login: Mutex<Option<LoginState>>,
    login_task: Mutex<Option<JoinHandle<()>>>,
    sync: Mutex<Option<SyncRun>>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Steam {
    pub fn new(token_file: PathBuf, contracts_file: PathBuf, manual_file: PathBuf) -> Steam {
        Steam {
            token_file,
            contracts_file,
            manual_file,
            login: Mutex::new(None),
            login_task: Mutex::new(None),
            sync: Mutex::new(None),
        }
    }

    pub fn login_state(&self) -> Option<LoginState> {
        lock(&self.login).clone()
    }

    pub fn is_syncing(&self) -> bool {
        lock(&self.sync).is_some()
    }

    pub fn cancel_login(&self) {
        if let Some(task) = lock(&self.login_task).take() {
            task.abort();
        }
        *lock(&self.login) = None;
    }

    pub fn logout(&self) -> Result<(), String> {
        self.cancel_login();
        store::delete_token(&self.token_file).map_err(|err| err.to_string())
    }

    /// Asks Steam for a QR code and returns once it is shown; approval in the Steam app is then awaited in the
    /// background, and the dialog follows it through `login_state`.
    pub async fn start_login(self: Arc<Self>) -> Result<(), String> {
        self.cancel_login();
        let server_list = ServerList::discover()
            .await
            .map_err(|err| format!("Steam: {err}"))?;
        let connection = UnAuthenticatedConnection::connect(&server_list)
            .await
            .map_err(|err| format!("Steam: {err}"))?;
        let platform_type = EAuthTokenPlatformType::k_EAuthTokenPlatformType_SteamClient;
        let begin = connection
            .service_method(CAuthentication_BeginAuthSessionViaQR_Request {
                device_friendly_name: Some(DEVICE_NAME.to_string()),
                platform_type: Some(EnumOrUnknown::new(platform_type)),
                website_id: Some("Client".to_string()),
                device_details: MessageField::some(CAuthentication_DeviceDetails {
                    device_friendly_name: Some(DEVICE_NAME.to_string()),
                    platform_type: Some(EnumOrUnknown::new(platform_type)),
                    os_type: Some(i32::from(Os::Windows)),
                    ..Default::default()
                }),
                ..Default::default()
            })
            .await
            .map_err(|err| format!("Steam: {err}"))?;
        *lock(&self.login) = Some(LoginState {
            status: LoginStatus::Waiting,
            qr: qr_data_url(begin.challenge_url())?,
            error: None,
        });

        let (client_id, request_id) = (begin.client_id(), begin.request_id().to_vec());
        let interval = Duration::from_secs_f32(begin.interval.unwrap_or(5.0).max(1.0));
        let steam = self.clone();
        let task = tauri::async_runtime::spawn(async move {
            let result = timeout(
                LOGIN_TIMEOUT,
                steam.wait_for_approval(&connection, client_id, request_id, interval),
            )
            .await;
            let failed = match result {
                Ok(Ok(())) => None,
                Ok(Err(err)) => Some(err),
                Err(_) => Some("The QR code expired. Try again.".to_string()),
            };
            let mut login = lock(&steam.login);
            match failed {
                // signed in: the dialog closes because there is nothing left to show
                None => *login = None,
                Some(error) => {
                    if let Some(state) = login.as_mut() {
                        state.status = LoginStatus::Failed;
                        state.error = Some(error);
                    }
                }
            }
        });
        *lock(&self.login_task) = Some(task);
        Ok(())
    }

    /// Polls until the user approves in the Steam app, following QR rotations, then saves the refresh token.
    async fn wait_for_approval(
        &self,
        connection: &UnAuthenticatedConnection,
        mut client_id: u64,
        request_id: Vec<u8>,
        interval: Duration,
    ) -> Result<(), String> {
        loop {
            sleep(interval).await;
            let poll = connection
                .service_method(CAuthentication_PollAuthSessionStatus_Request {
                    client_id: Some(client_id),
                    request_id: Some(request_id.clone()),
                    ..Default::default()
                })
                .await
                .map_err(|err| format!("Steam: {err}"))?;
            if let Some(new_client_id) = poll.new_client_id {
                client_id = new_client_id;
            }
            if let Some(refresh_token) = poll.refresh_token {
                return store::save_token(&self.token_file, &refresh_token)
                    .map_err(|err| format!("Could not save the sign-in: {err}"));
            }
            let mut login = lock(&self.login);
            if let Some(state) = login.as_mut() {
                if let Some(url) = &poll.new_challenge_url {
                    state.qr = qr_data_url(url)?;
                }
                if poll.had_remote_interaction() {
                    state.status = LoginStatus::Scanned;
                }
            }
        }
    }

    /// Runs one sync, or joins the one already running.
    pub async fn sync(self: Arc<Self>) -> Result<(), String> {
        let run = {
            let mut current = lock(&self.sync);
            current
                .get_or_insert_with(|| {
                    let steam = self.clone();
                    let run: Pin<Box<dyn Future<Output = Result<(), String>> + Send>> =
                        Box::pin(async move {
                            let result = guarded(steam.clone().sync_owned()).await;
                            *lock(&steam.sync) = None;
                            result
                        });
                    run.shared()
                })
                .clone()
        };
        run.await
    }

    async fn sync_owned(self: Arc<Self>) -> Result<(), String> {
        self.sync_contracts().await
    }

    async fn sync_contracts(&self) -> Result<(), String> {
        let nodes = timeout(SYNC_TIMEOUT, self.completed_nodes())
            .await
            .map_err(|_| {
                "Steam or the TF2 Game Coordinator did not respond in time.".to_string()
            })??;
        let completed: Vec<String> = MAPS
            .iter()
            .filter(|map| nodes.contains(&map.contract_node))
            .map(|map| map.code.clone())
            .collect();
        store::write_contracts(
            &self.contracts_file,
            &Contracts {
                completed: completed.clone(),
                synced_at: iso_now(),
            },
        )
        .map_err(|err| err.to_string())?;
        // manual marks only stand in until Steam confirms them
        let manual = store::read_manual(&self.manual_file).map_err(|err| err.to_string())?;
        let still_manual: Vec<String> = manual
            .iter()
            .filter(|code| !completed.contains(code))
            .cloned()
            .collect();
        if still_manual.len() != manual.len() {
            store::write_manual(&self.manual_file, &still_manual).map_err(|err| err.to_string())?;
        }
        Ok(())
    }

    /// Logs in with the saved token, says hello to the TF2 Game Coordinator and returns the ConTracker node
    /// defindexes whose contract is completed.
    async fn completed_nodes(&self) -> Result<BTreeSet<u32>, String> {
        if process::tf2_running() == Some(true) {
            return Err("TF2 is running. Close it, then sync again.".to_string());
        }
        if !store::has_token(&self.token_file) {
            return Err("Not signed in to Steam. Use \"Sign in with Steam\" first.".to_string());
        }
        let raw = store::read_token(&self.token_file).map_err(|err| err.to_string())?;
        let Ok(token) = RefreshToken::new(raw) else {
            let _ = store::delete_token(&self.token_file);
            return Err(SIGN_IN_EXPIRED.to_string());
        };
        let server_list = ServerList::discover()
            .await
            .map_err(|err| format!("Steam: {err}"))?;
        let connection = match Connection::login_with_refresh_token(&server_list, &token).await {
            Ok(connection) => connection,
            Err(err) if is_sign_in_rejected(&err) => {
                // the token is dead for good (signed out everywhere, password changed): ask for a new sign-in
                let _ = store::delete_token(&self.token_file);
                return Err(SIGN_IN_EXPIRED.to_string());
            }
            Err(err) => return Err(format!("Steam: {err}")),
        };

        // Never take over a game session that is in use: it would kick you out of TF2. Without an answer from
        // Steam there is no way to know, so the sync stops.
        match playing_state(&connection).await? {
            Some(state) if state.playing_blocked() => {
                return Err(format!(
                    "Your account is in a game elsewhere (app {}). Close TF2, then sync again.",
                    state.playing_app()
                ));
            }
            Some(_) => {}
            None => {
                return Err(
                    "Steam did not say whether your account is in a game. Try again.".to_string(),
                )
            }
        }

        let owner: u64 = connection.steam_id().into();
        let gc = GameCoordinator::new(&connection, APP_ID)
            .await
            .map_err(|err| format!("TF2 Game Coordinator: {err}"))?;
        let cache = wait_for_cache(&gc, owner).await?;
        let mut nodes = BTreeSet::new();
        for object in cache
            .objects
            .iter()
            .filter(|object| object.type_id() == QUEST_MAP_NODE_TYPE)
        {
            for data in &object.object_data {
                let node = CSOQuestMapNode::parse(&mut Cursor::new(data))
                    .map_err(|err| err.to_string())?;
                if node.star_0_earned() {
                    nodes.insert(node.defindex());
                }
            }
        }
        Ok(nodes)
    }
}

/// Runs Steam work on its own task, so a panic deep in a library becomes an error message instead of a call that
/// never answers (which left the Sync button spinning).
pub async fn guarded<T: Send + 'static>(
    work: impl Future<Output = Result<T, String>> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn(work)
        .await
        .unwrap_or_else(|err| Err(format!("Steam stopped unexpectedly: {err}")))
}

/// Steam's answer when the stored refresh token is no longer accepted.
fn is_sign_in_rejected(err: &ConnectionError) -> bool {
    let result = match err {
        ConnectionError::LoginError(LoginError::AccessToken(RefreshTokenError::Expired)) => {
            return true
        }
        ConnectionError::LoginError(LoginError::Unknown(result)) => result,
        ConnectionError::Network(NetworkError::ApiError(result)) => result,
        _ => return false,
    };
    matches!(
        result,
        EResult::AccessDenied | EResult::Expired | EResult::Revoked | EResult::InvalidPassword
    )
}

/// Steam's playing-state message sent right after logon, or None if it does not arrive in time.
async fn playing_state(
    connection: &Connection,
) -> Result<Option<CMsgClientPlayingSessionState>, String> {
    // subscribe before checking what already arrived, so a message landing in between is not missed
    let mut incoming = pin!(connection.on::<CMsgClientPlayingSessionState>());
    let kind = MsgKind::from(EMsg::k_EMsgClientPlayingSessionState);
    for raw in connection.take_unprocessed() {
        if raw.kind == kind {
            return raw.into_message().map(Some).map_err(|err| err.to_string());
        }
    }
    match timeout(PLAYING_STATE_WAIT, incoming.next()).await {
        Ok(Some(state)) => state.map(Some).map_err(|err| err.to_string()),
        Ok(None) | Err(_) => Ok(None),
    }
}

fn cache_refresh(owner: u64) -> CMsgSOCacheSubscriptionRefresh {
    CMsgSOCacheSubscriptionRefresh {
        owner: Some(owner),
        ..Default::default()
    }
}

/// Answers the Game Coordinator's cache checks and waits for the cache owned by `owner`.
async fn wait_for_cache(gc: &GameCoordinator, owner: u64) -> Result<CMsgSOCacheSubscribed, String> {
    let mut checks = pin!(gc.on::<CMsgSOCacheSubscriptionCheck>());
    let mut subscribed = pin!(gc.on::<CMsgSOCacheSubscribed>());
    gc.send(cache_refresh(owner))
        .await
        .map_err(|err| err.to_string())?;
    loop {
        tokio::select! {
            Some(check) = checks.next() => {
                check.map_err(|err| err.to_string())?;
                gc.send(cache_refresh(owner)).await.map_err(|err| err.to_string())?;
            }
            Some(cache) = subscribed.next() => {
                let cache = cache.map_err(|err| err.to_string())?;
                let matches = match (cache.owner, cache.owner_soid.as_ref()) {
                    (Some(id), _) => id == owner,
                    (None, Some(soid)) => soid.id() == owner,
                    (None, None) => true,
                };
                if matches {
                    return Ok(cache);
                }
            }
            else => return Err("The TF2 Game Coordinator closed the connection.".to_string()),
        }
    }
}

fn qr_data_url(url: &str) -> Result<String, String> {
    let code = QrCode::new(url).map_err(|err| err.to_string())?;
    let svg = code
        .render::<svg::Color<'_>>()
        .min_dimensions(240, 240)
        .quiet_zone(true)
        .build();
    // a data URL needs only these few characters escaped for an SVG
    let escaped: String = svg
        .chars()
        .map(|c| match c {
            '%' => "%25".to_string(),
            '#' => "%23".to_string(),
            '<' => "%3C".to_string(),
            '>' => "%3E".to_string(),
            '"' => "'".to_string(),
            '\n' | '\r' => " ".to_string(),
            c => c.to_string(),
        })
        .collect();
    Ok(format!("data:image/svg+xml;charset=utf-8,{escaped}"))
}

/// The current UTC time as an ISO 8601 string with milliseconds, like JavaScript's toISOString.
fn iso_now() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    iso_from_millis(ms)
}

fn iso_from_millis(ms: u64) -> String {
    let (secs, millis) = (ms / 1000, ms % 1000);
    let (days, rest) = (secs / 86_400, secs % 86_400);
    // civil-from-days (Howard Hinnant's algorithm)
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_timestamps_match_javascript() {
        assert_eq!(iso_from_millis(0), "1970-01-01T00:00:00.000Z");
        // new Date(Date.UTC(2026, 9, 17, 12, 27, 2, 916)).toISOString()
        assert_eq!(
            iso_from_millis(1_792_240_022_916),
            "2026-10-17T12:27:02.916Z"
        );
        assert_eq!(iso_from_millis(951_782_400_000), "2000-02-29T00:00:00.000Z");
    }

    #[test]
    fn qr_code_is_an_svg_data_url() {
        let url = qr_data_url("https://s.team/q/1/2").unwrap();
        assert!(url.starts_with("data:image/svg+xml;charset=utf-8,%3C"));
        assert!(!url.contains('#') && !url.contains('<'));
    }
}
