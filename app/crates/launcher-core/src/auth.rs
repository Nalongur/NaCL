use crate::data::{AppPaths, DataError};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tempfile::NamedTempFile;
use url::Url;
use uuid::Uuid;

pub const MICROSOFT_CLIENT_ID: &str = "0dc33f88-d247-4107-a9cc-8fb319e90d47";
const ACCOUNT_SCHEMA_VERSION: u32 = 1;
const MICROSOFT_SCOPE: &str = "XboxLive.signin offline_access";
const MICROSOFT_AUTHORIZE_URL: &str =
    "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize";
const MICROSOFT_TOKEN_URL: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const XBOX_USER_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MINECRAFT_LOGIN_URL: &str =
    "https://api.minecraftservices.com/authentication/login_with_xbox";
const MINECRAFT_ENTITLEMENTS_URL: &str = "https://api.minecraftservices.com/entitlements/mcstore";
const MINECRAFT_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";
const LOGIN_TIMEOUT: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinecraftSkin {
    pub id: String,
    pub state: String,
    pub url: String,
    pub variant: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinecraftCape {
    pub id: String,
    pub state: String,
    pub url: String,
    pub alias: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MicrosoftAccount {
    pub schema_version: u32,
    pub minecraft_id: String,
    pub minecraft_name: String,
    pub skins: Vec<MinecraftSkin>,
    pub capes: Vec<MinecraftCape>,
    pub signed_in_epoch_ms: u128,
}

impl MicrosoftAccount {
    pub fn active_skin(&self) -> Option<&MinecraftSkin> {
        self.skins
            .iter()
            .find(|skin| skin.state.eq_ignore_ascii_case("active"))
            .or_else(|| self.skins.first())
    }

    pub fn active_cape(&self) -> Option<&MinecraftCape> {
        self.capes
            .iter()
            .find(|cape| cape.state.eq_ignore_ascii_case("active"))
    }
}

#[derive(Debug, Clone)]
pub struct OnlineLaunchIdentity {
    pub username: String,
    pub uuid: String,
    pub access_token: String,
    pub client_id: String,
    pub xuid: String,
}

#[derive(Debug)]
pub struct MicrosoftLoginSession {
    listener: TcpListener,
    verifier: String,
    state: String,
    redirect_uri: String,
    authorization_url: String,
}

impl MicrosoftLoginSession {
    pub fn authorization_url(&self) -> &str {
        &self.authorization_url
    }
}

#[derive(Debug)]
pub enum AuthError {
    Data(DataError),
    Io {
        action: &'static str,
        source: io::Error,
    },
    Url(url::ParseError),
    Network(String),
    Service {
        stage: &'static str,
        status: u16,
        message: String,
    },
    InvalidCallback,
    StateMismatch,
    LoginTimedOut,
    LoginCancelled(String),
    MissingRefreshToken,
    MissingXboxClaim,
    MissingEntitlement,
    MissingMinecraftProfile,
    InvalidStoredAccount,
    UnsupportedPlatform,
}

impl fmt::Display for AuthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Data(error) => error.fmt(formatter),
            Self::Io { action, source } => write!(formatter, "无法{action}：{source}"),
            Self::Url(error) => write!(formatter, "登录地址无效：{error}"),
            Self::Network(error) => write!(formatter, "登录网络请求失败：{error}"),
            Self::Service {
                stage,
                status,
                message,
            } => {
                write!(formatter, "{stage}失败（HTTP {status}）：{message}")
            }
            Self::InvalidCallback => write!(formatter, "Microsoft 登录回调无效"),
            Self::StateMismatch => write!(formatter, "Microsoft 登录状态校验失败，请重新登录"),
            Self::LoginTimedOut => write!(formatter, "Microsoft 登录等待超时，请重新登录"),
            Self::LoginCancelled(message) => write!(formatter, "Microsoft 登录未完成：{message}"),
            Self::MissingRefreshToken => write!(formatter, "Microsoft 未返回刷新令牌，请重新授权"),
            Self::MissingXboxClaim => write!(formatter, "Xbox 身份响应缺少必要的用户信息"),
            Self::MissingEntitlement => {
                write!(formatter, "该账号没有 Minecraft Java Edition 游戏资格")
            }
            Self::MissingMinecraftProfile => {
                write!(formatter, "该账号尚未创建 Minecraft Java Edition 档案")
            }
            Self::InvalidStoredAccount => {
                write!(formatter, "本地 Microsoft 账号数据无效，请重新登录")
            }
            Self::UnsupportedPlatform => write!(formatter, "当前平台不支持安全保存 Microsoft 登录"),
        }
    }
}

impl std::error::Error for AuthError {}

impl From<DataError> for AuthError {
    fn from(error: DataError) -> Self {
        Self::Data(error)
    }
}

impl From<url::ParseError> for AuthError {
    fn from(error: url::ParseError) -> Self {
        Self::Url(error)
    }
}

#[derive(Debug, Deserialize)]
struct MicrosoftTokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct XboxTokenResponse {
    token: String,
    display_claims: XboxDisplayClaims,
}

#[derive(Debug, Deserialize)]
struct XboxDisplayClaims {
    #[serde(default)]
    xui: Vec<XboxUserClaim>,
}

#[derive(Debug, Deserialize)]
struct XboxUserClaim {
    uhs: String,
    #[serde(default)]
    xid: String,
}

#[derive(Debug, Deserialize)]
struct MinecraftTokenResponse {
    access_token: String,
    expires_in: u64,
}

#[derive(Debug, Deserialize)]
struct MinecraftEntitlements {
    #[serde(default)]
    items: Vec<MinecraftEntitlement>,
}

#[derive(Debug, Deserialize)]
struct MinecraftEntitlement {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize)]
struct MinecraftProfileResponse {
    id: String,
    name: String,
    #[serde(default)]
    skins: Vec<MinecraftSkin>,
    #[serde(default)]
    capes: Vec<MinecraftCape>,
}

#[derive(Debug, Deserialize)]
struct ServiceErrorResponse {
    #[serde(default)]
    error: String,
    #[serde(default)]
    error_description: String,
    #[serde(default, rename = "errorMessage")]
    error_message: String,
    #[serde(default, rename = "XErr")]
    xerr: Option<u64>,
    #[serde(default, rename = "Redirect")]
    redirect: String,
}

#[derive(Debug, Clone)]
struct CachedSession {
    identity: OnlineLaunchIdentity,
    expires_epoch_seconds: u64,
}

fn session_cache() -> &'static Mutex<Option<CachedSession>> {
    static CACHE: OnceLock<Mutex<Option<CachedSession>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

pub fn begin_microsoft_login() -> Result<MicrosoftLoginSession, AuthError> {
    let listener = TcpListener::bind("localhost:0").map_err(|source| AuthError::Io {
        action: "启动本地登录回调",
        source,
    })?;
    listener
        .set_nonblocking(true)
        .map_err(|source| AuthError::Io {
            action: "配置本地登录回调",
            source,
        })?;
    let port = listener
        .local_addr()
        .map_err(|source| AuthError::Io {
            action: "读取本地登录端口",
            source,
        })?
        .port();
    let redirect_uri = format!("http://localhost:{port}/");
    let verifier = format!(
        "{}{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let mut authorization_url = Url::parse(MICROSOFT_AUTHORIZE_URL)?;
    authorization_url
        .query_pairs_mut()
        .append_pair("client_id", MICROSOFT_CLIENT_ID)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("response_mode", "query")
        .append_pair("scope", MICROSOFT_SCOPE)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &state)
        .append_pair("prompt", "select_account");
    Ok(MicrosoftLoginSession {
        listener,
        verifier,
        state,
        redirect_uri,
        authorization_url: authorization_url.to_string(),
    })
}

pub fn complete_microsoft_login(
    session: MicrosoftLoginSession,
) -> Result<MicrosoftAccount, AuthError> {
    let code = wait_for_callback(&session)?;
    let client = auth_client()?;
    let token = exchange_authorization_code(&client, &session, &code)?;
    let refresh_token = token.refresh_token.ok_or(AuthError::MissingRefreshToken)?;
    let (account, cached) = authenticate_minecraft(&client, &token.access_token)?;
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    save_refresh_token(&paths, refresh_token.as_bytes())?;
    save_account(&paths, &account)?;
    *session_cache()
        .lock()
        .map_err(|_| AuthError::InvalidStoredAccount)? = Some(cached);
    Ok(account)
}

pub fn load_microsoft_account() -> Result<Option<MicrosoftAccount>, AuthError> {
    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    let path = account_path(&paths);
    if !path.is_file() || !refresh_token_path(&paths).is_file() {
        return Ok(None);
    }
    let contents = fs::read_to_string(&path).map_err(|source| AuthError::Io {
        action: "读取 Microsoft 账号档案",
        source,
    })?;
    let account = serde_json::from_str::<MicrosoftAccount>(&contents)
        .map_err(|_| AuthError::InvalidStoredAccount)?;
    if account.schema_version != ACCOUNT_SCHEMA_VERSION
        || account.minecraft_id.is_empty()
        || account.minecraft_name.is_empty()
    {
        return Err(AuthError::InvalidStoredAccount);
    }
    Ok(Some(account))
}

pub fn logout_microsoft() -> Result<(), AuthError> {
    let paths = AppPaths::resolve()?;
    for path in [account_path(&paths), refresh_token_path(&paths)] {
        if path.is_file() {
            fs::remove_file(&path).map_err(|source| AuthError::Io {
                action: "删除 Microsoft 登录数据",
                source,
            })?;
        }
    }
    *session_cache()
        .lock()
        .map_err(|_| AuthError::InvalidStoredAccount)? = None;
    Ok(())
}

pub fn online_launch_identity() -> Result<OnlineLaunchIdentity, AuthError> {
    let now = now_epoch_seconds();
    if let Some(cached) = session_cache()
        .lock()
        .map_err(|_| AuthError::InvalidStoredAccount)?
        .as_ref()
        .filter(|cached| cached.expires_epoch_seconds > now.saturating_add(300))
        .cloned()
    {
        return Ok(cached.identity);
    }

    let paths = AppPaths::resolve()?;
    paths.initialize()?;
    let encrypted = fs::read(refresh_token_path(&paths)).map_err(|source| AuthError::Io {
        action: "读取 Microsoft 登录凭据",
        source,
    })?;
    let refresh_token = String::from_utf8(unprotect_secret(&encrypted)?)
        .map_err(|_| AuthError::InvalidStoredAccount)?;
    let client = auth_client()?;
    let token = refresh_microsoft_token(&client, &refresh_token)?;
    if let Some(rotated) = token.refresh_token.as_ref() {
        save_refresh_token(&paths, rotated.as_bytes())?;
    }
    let (account, cached) = authenticate_minecraft(&client, &token.access_token)?;
    save_account(&paths, &account)?;
    let identity = cached.identity.clone();
    *session_cache()
        .lock()
        .map_err(|_| AuthError::InvalidStoredAccount)? = Some(cached);
    Ok(identity)
}

fn auth_client() -> Result<Client, AuthError> {
    Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(45))
        .user_agent(format!("NaCL/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| AuthError::Network(error.to_string()))
}

fn wait_for_callback(session: &MicrosoftLoginSession) -> Result<String, AuthError> {
    let deadline = Instant::now() + LOGIN_TIMEOUT;
    loop {
        match session.listener.accept() {
            Ok((mut stream, peer)) => {
                if !peer.ip().is_loopback() {
                    continue;
                }
                stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
                let mut buffer = [0_u8; 8192];
                let read = stream.read(&mut buffer).map_err(|source| AuthError::Io {
                    action: "读取 Microsoft 登录回调",
                    source,
                })?;
                let request = String::from_utf8_lossy(&buffer[..read]);
                let target = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .ok_or(AuthError::InvalidCallback)?;
                let callback = Url::parse(&format!("http://localhost{target}"))?;
                let query = callback
                    .query_pairs()
                    .collect::<std::collections::HashMap<_, _>>();
                let state = query.get("state").ok_or(AuthError::InvalidCallback)?;
                if state.as_ref() != session.state {
                    write_callback_page(&mut stream, false);
                    return Err(AuthError::StateMismatch);
                }
                if let Some(error) = query.get("error") {
                    let description = query
                        .get("error_description")
                        .map_or_else(|| error.to_string(), ToString::to_string);
                    write_callback_page(&mut stream, false);
                    return Err(AuthError::LoginCancelled(description));
                }
                let code = query
                    .get("code")
                    .map(ToString::to_string)
                    .ok_or(AuthError::InvalidCallback)?;
                write_callback_page(&mut stream, true);
                return Ok(code);
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err(AuthError::LoginTimedOut);
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(source) => {
                return Err(AuthError::Io {
                    action: "等待 Microsoft 登录回调",
                    source,
                });
            }
        }
    }
}

fn write_callback_page(stream: &mut impl Write, success: bool) {
    let (title, message) = if success {
        (
            "NaCL 登录完成",
            "Microsoft 已将授权结果安全返回 NaCL，可以关闭此页面。",
        )
    } else {
        ("NaCL 登录未完成", "请返回 NaCL 查看详细信息并重新尝试。")
    };
    let body = format!(
        "<!doctype html><html lang=\"zh-CN\"><meta charset=\"utf-8\"><title>{title}</title><style>body{{font-family:Segoe UI,Microsoft YaHei UI,sans-serif;background:#182022;color:#eef6f6;display:grid;place-items:center;min-height:100vh;margin:0}}main{{max-width:520px;padding:40px}}h1{{font-size:28px}}p{{color:#a9b9ba;line-height:1.7}}</style><main><h1>{title}</h1><p>{message}</p></main></html>"
    );
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
}

fn exchange_authorization_code(
    client: &Client,
    session: &MicrosoftLoginSession,
    code: &str,
) -> Result<MicrosoftTokenResponse, AuthError> {
    post_form(
        client,
        MICROSOFT_TOKEN_URL,
        &[
            ("client_id", MICROSOFT_CLIENT_ID),
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &session.redirect_uri),
            ("code_verifier", &session.verifier),
            ("scope", MICROSOFT_SCOPE),
        ],
        "Microsoft 令牌交换",
    )
}

fn refresh_microsoft_token(
    client: &Client,
    refresh_token: &str,
) -> Result<MicrosoftTokenResponse, AuthError> {
    post_form(
        client,
        MICROSOFT_TOKEN_URL,
        &[
            ("client_id", MICROSOFT_CLIENT_ID),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("scope", MICROSOFT_SCOPE),
        ],
        "Microsoft 登录刷新",
    )
}

fn post_form<T: for<'de> Deserialize<'de>>(
    client: &Client,
    url: &str,
    form: &[(&str, &str)],
    stage: &'static str,
) -> Result<T, AuthError> {
    let response = client
        .post(url)
        .form(form)
        .send()
        .map_err(|error| AuthError::Network(error.to_string()))?;
    parse_response(response, stage)
}

fn authenticate_minecraft(
    client: &Client,
    microsoft_access_token: &str,
) -> Result<(MicrosoftAccount, CachedSession), AuthError> {
    let xbox: XboxTokenResponse = post_json(
        client,
        XBOX_USER_URL,
        &serde_json::json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={microsoft_access_token}")
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        }),
        "Xbox 用户认证",
    )?;
    let xsts: XboxTokenResponse = post_json(
        client,
        XSTS_URL,
        &serde_json::json!({
            "Properties": {
                "SandboxId": "RETAIL",
                "UserTokens": [xbox.token]
            },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        }),
        "Xbox XSTS 认证",
    )?;
    let claim = xsts
        .display_claims
        .xui
        .first()
        .ok_or(AuthError::MissingXboxClaim)?;
    let minecraft: MinecraftTokenResponse = post_json(
        client,
        MINECRAFT_LOGIN_URL,
        &serde_json::json!({
            "identityToken": format!("XBL3.0 x={};{}", claim.uhs, xsts.token)
        }),
        "Minecraft Services 登录",
    )?;
    let entitlements: MinecraftEntitlements = get_bearer(
        client,
        MINECRAFT_ENTITLEMENTS_URL,
        &minecraft.access_token,
        "Minecraft 游戏资格检查",
    )?;
    if !entitlements
        .items
        .iter()
        .any(|item| item.name == "game_minecraft" || item.name == "product_minecraft")
    {
        return Err(AuthError::MissingEntitlement);
    }
    let profile: MinecraftProfileResponse = get_bearer(
        client,
        MINECRAFT_PROFILE_URL,
        &minecraft.access_token,
        "Minecraft 档案读取",
    )?;
    if profile.id.is_empty() || profile.name.is_empty() {
        return Err(AuthError::MissingMinecraftProfile);
    }
    let skins = profile
        .skins
        .into_iter()
        .map(|mut skin| {
            skin.url = secure_texture_url(&skin.url);
            skin
        })
        .collect();
    let capes = profile
        .capes
        .into_iter()
        .map(|mut cape| {
            cape.url = secure_texture_url(&cape.url);
            cape
        })
        .collect();
    let account = MicrosoftAccount {
        schema_version: ACCOUNT_SCHEMA_VERSION,
        minecraft_id: profile.id.clone(),
        minecraft_name: profile.name.clone(),
        skins,
        capes,
        signed_in_epoch_ms: now_epoch_ms(),
    };
    let cached = CachedSession {
        identity: OnlineLaunchIdentity {
            username: profile.name,
            uuid: profile.id,
            access_token: minecraft.access_token,
            client_id: MICROSOFT_CLIENT_ID.to_string(),
            xuid: claim.xid.clone(),
        },
        expires_epoch_seconds: now_epoch_seconds().saturating_add(minecraft.expires_in),
    };
    Ok((account, cached))
}

fn post_json<T: for<'de> Deserialize<'de>>(
    client: &Client,
    url: &str,
    body: &serde_json::Value,
    stage: &'static str,
) -> Result<T, AuthError> {
    let response = client
        .post(url)
        .header("x-xbl-contract-version", "1")
        .json(body)
        .send()
        .map_err(|error| AuthError::Network(error.to_string()))?;
    parse_response(response, stage)
}

fn get_bearer<T: for<'de> Deserialize<'de>>(
    client: &Client,
    url: &str,
    token: &str,
    stage: &'static str,
) -> Result<T, AuthError> {
    let response = client
        .get(url)
        .bearer_auth(token)
        .send()
        .map_err(|error| AuthError::Network(error.to_string()))?;
    parse_response(response, stage)
}

fn parse_response<T: for<'de> Deserialize<'de>>(
    response: reqwest::blocking::Response,
    stage: &'static str,
) -> Result<T, AuthError> {
    let status = response.status();
    let body = response
        .text()
        .map_err(|error| AuthError::Network(error.to_string()))?;
    if !status.is_success() {
        let message = serde_json::from_str::<ServiceErrorResponse>(&body)
            .map(service_error_message)
            .unwrap_or_else(|_| "服务未返回可读取的错误信息".to_string());
        return Err(AuthError::Service {
            stage,
            status: status.as_u16(),
            message,
        });
    }
    serde_json::from_str(&body).map_err(|_| AuthError::Service {
        stage,
        status: status.as_u16(),
        message: "服务响应格式无效".to_string(),
    })
}

fn service_error_message(error: ServiceErrorResponse) -> String {
    let mut message = if !error.error_description.is_empty() {
        error.error_description
    } else if !error.error_message.is_empty() {
        error.error_message
    } else if !error.error.is_empty() {
        error.error
    } else {
        "认证被服务拒绝".to_string()
    };
    if let Some(xerr) = error.xerr {
        message.push_str(&format!("（Xbox 错误 {xerr}）"));
    }
    if !error.redirect.is_empty() {
        message.push_str("；请按 Xbox 提示完成账号设置");
    }
    message.chars().take(500).collect()
}

fn secure_texture_url(url: &str) -> String {
    url.strip_prefix("http://textures.minecraft.net/")
        .map_or_else(
            || url.to_string(),
            |suffix| format!("https://textures.minecraft.net/{suffix}"),
        )
}

fn account_path(paths: &AppPaths) -> PathBuf {
    paths.config_dir.join("microsoft-account.json")
}

fn refresh_token_path(paths: &AppPaths) -> PathBuf {
    paths.config_dir.join("microsoft-refresh-token.bin")
}

fn save_account(paths: &AppPaths, account: &MicrosoftAccount) -> Result<(), AuthError> {
    let bytes = serde_json::to_vec_pretty(account).map_err(|_| AuthError::InvalidStoredAccount)?;
    save_atomically(
        &paths.config_dir,
        &account_path(paths),
        &bytes,
        "保存 Microsoft 账号档案",
    )
}

fn save_refresh_token(paths: &AppPaths, token: &[u8]) -> Result<(), AuthError> {
    let encrypted = protect_secret(token)?;
    save_atomically(
        &paths.config_dir,
        &refresh_token_path(paths),
        &encrypted,
        "保存 Microsoft 登录凭据",
    )
}

fn save_atomically(
    directory: &Path,
    destination: &Path,
    contents: &[u8],
    action: &'static str,
) -> Result<(), AuthError> {
    fs::create_dir_all(directory).map_err(|source| AuthError::Io { action, source })?;
    let mut temporary =
        NamedTempFile::new_in(directory).map_err(|source| AuthError::Io { action, source })?;
    temporary
        .write_all(contents)
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|source| AuthError::Io { action, source })?;
    temporary
        .persist(destination)
        .map_err(|error| AuthError::Io {
            action,
            source: error.error,
        })?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn protect_secret(secret: &[u8]) -> Result<Vec<u8>, AuthError> {
    use std::ptr;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: secret.len() as u32,
        pbData: secret.as_ptr() as *mut u8,
    };
    let entropy_bytes = b"NaCL Microsoft refresh token v1";
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: entropy_bytes.len() as u32,
        pbData: entropy_bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };
    let success = unsafe {
        CryptProtectData(
            &input,
            ptr::null(),
            &entropy,
            ptr::null(),
            ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if success == 0 {
        return Err(AuthError::Io {
            action: "使用 Windows 加密登录凭据",
            source: io::Error::last_os_error(),
        });
    }
    let encrypted =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe {
        LocalFree(output.pbData as *mut _);
    }
    Ok(encrypted)
}

#[cfg(target_os = "windows")]
fn unprotect_secret(encrypted: &[u8]) -> Result<Vec<u8>, AuthError> {
    use std::ptr;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let input = CRYPT_INTEGER_BLOB {
        cbData: encrypted.len() as u32,
        pbData: encrypted.as_ptr() as *mut u8,
    };
    let entropy_bytes = b"NaCL Microsoft refresh token v1";
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: entropy_bytes.len() as u32,
        pbData: entropy_bytes.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };
    let success = unsafe {
        CryptUnprotectData(
            &input,
            ptr::null_mut(),
            &entropy,
            ptr::null(),
            ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if success == 0 {
        return Err(AuthError::Io {
            action: "使用 Windows 解密登录凭据",
            source: io::Error::last_os_error(),
        });
    }
    let secret =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe {
        LocalFree(output.pbData as *mut _);
    }
    Ok(secret)
}

#[cfg(not(target_os = "windows"))]
fn protect_secret(_secret: &[u8]) -> Result<Vec<u8>, AuthError> {
    Err(AuthError::UnsupportedPlatform)
}

#[cfg(not(target_os = "windows"))]
fn unprotect_secret(_encrypted: &[u8]) -> Result<Vec<u8>, AuthError> {
    Err(AuthError::UnsupportedPlatform)
}

fn now_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn now_epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

#[cfg(test)]
mod tests {
    use super::{service_error_message, ServiceErrorResponse};

    #[test]
    fn service_errors_do_not_echo_response_json() {
        let message = service_error_message(ServiceErrorResponse {
            error: "invalid_grant".to_string(),
            error_description: "authorization expired".to_string(),
            error_message: String::new(),
            xerr: None,
            redirect: String::new(),
        });
        assert_eq!(message, "authorization expired");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_dpapi_round_trip_is_user_scoped() {
        let secret = b"temporary-test-refresh-token";
        let encrypted = super::protect_secret(secret).expect("DPAPI should encrypt");
        assert_ne!(encrypted, secret);
        let decrypted = super::unprotect_secret(&encrypted).expect("DPAPI should decrypt");
        assert_eq!(decrypted, secret);
    }
}
