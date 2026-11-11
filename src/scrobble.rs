use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const LASTFM_API_URL: &str = "https://ws.audioscrobbler.com/2.0/";
pub const LIBREFM_API_URL: &str = "https://libre.fm/2.0/";
pub const LASTFM_AUTH_URL: &str = "https://www.last.fm/api/auth/";
pub const LIBREFM_AUTH_URL: &str = "https://libre.fm/api/auth/";

// API key/secret dapat di-embed saat build:
//   LASTFM_API_KEY=xxx LASTFM_API_SECRET=yyy cargo build --release
// Jika tidak di-set saat build, fallback ke nilai di ~/.morp/scrobble.toml
const BUILTIN_LASTFM_API_KEY: Option<&str> = option_env!("LASTFM_API_KEY");
const BUILTIN_LASTFM_API_SECRET: Option<&str> = option_env!("LASTFM_API_SECRET");
const BUILTIN_LIBREFM_API_KEY: Option<&str> = option_env!("LIBREFM_API_KEY");
const BUILTIN_LIBREFM_API_SECRET: Option<&str> = option_env!("LIBREFM_API_SECRET");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrobbleConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_service")]
    pub service: String,
    /// Hanya diperlukan jika build tidak menyertakan LASTFM_API_KEY/LIBREFM_API_KEY
    #[serde(default)]
    pub api_key: String,
    /// Hanya diperlukan jika build tidak menyertakan LASTFM_API_SECRET/LIBREFM_API_SECRET
    #[serde(default)]
    pub api_secret: String,
    #[serde(default)]
    pub session_key: String,
    #[serde(default)]
    pub username: String,
}

fn default_service() -> String {
    "lastfm".to_string()
}

impl Default for ScrobbleConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            service: default_service(),
            api_key: String::new(),
            api_secret: String::new(),
            session_key: String::new(),
            username: String::new(),
        }
    }
}

impl ScrobbleConfig {
    pub fn config_path() -> PathBuf {
        if let Ok(morp_dir) = std::env::var("MORP_DIR") {
            PathBuf::from(morp_dir).join("scrobble.toml")
        } else if let Some(home) = dirs::home_dir() {
            home.join(".morp").join("scrobble.toml")
        } else {
            PathBuf::from("/tmp/morp-scrobble.toml")
        }
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if let Ok(content) = std::fs::read_to_string(&path) {
            toml::from_str(&content).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let content = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, content).map_err(|e| e.to_string())
    }

    pub fn api_url(&self) -> &'static str {
        if self.service.to_lowercase() == "librefm" {
            LIBREFM_API_URL
        } else {
            LASTFM_API_URL
        }
    }

    pub fn browser_auth_base_url(&self) -> &'static str {
        if self.service.to_lowercase() == "librefm" {
            LIBREFM_AUTH_URL
        } else {
            LASTFM_AUTH_URL
        }
    }

    pub fn effective_api_key(&self) -> &str {
        let builtin = if self.service.to_lowercase() == "librefm" {
            BUILTIN_LIBREFM_API_KEY
        } else {
            BUILTIN_LASTFM_API_KEY
        };
        builtin.unwrap_or(self.api_key.as_str())
    }

    pub fn effective_api_secret(&self) -> &str {
        let builtin = if self.service.to_lowercase() == "librefm" {
            BUILTIN_LIBREFM_API_SECRET
        } else {
            BUILTIN_LASTFM_API_SECRET
        };
        builtin.unwrap_or(self.api_secret.as_str())
    }

    pub fn is_ready(&self) -> bool {
        self.enabled
            && !self.effective_api_key().is_empty()
            && !self.session_key.is_empty()
    }

    pub fn can_authenticate(&self) -> bool {
        !self.effective_api_key().is_empty() && !self.effective_api_secret().is_empty()
    }
}

pub enum ScrobbleEvent {
    NowPlaying {
        artist: String,
        title: String,
        album: String,
        duration: Option<u64>,
    },
    Scrobble {
        artist: String,
        title: String,
        album: String,
        duration: Option<u64>,
        timestamp: u64,
    },
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn compute_sig(params: &BTreeMap<String, String>, secret: &str) -> String {
    let mut s = String::new();
    for (k, v) in params {
        s.push_str(k);
        s.push_str(v);
    }
    s.push_str(secret);
    format!("{:x}", md5::compute(s.as_bytes()))
}

#[allow(dead_code)]
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
            | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            b => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

#[derive(Debug, Deserialize)]
struct LastfmSession {
    key: String,
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LastfmAuthResponse {
    session: Option<LastfmSession>,
    error: Option<u32>,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    token: Option<String>,
    error: Option<u32>,
    message: Option<String>,
}

/// Step 1: Minta token sementara dari Last.fm/Libre.fm.
pub fn get_token(config: &ScrobbleConfig) -> Result<String, String> {
    let api_key = config.effective_api_key();
    let api_secret = config.effective_api_secret();

    if api_key.is_empty() {
        return Err("API key tidak tersedia".to_string());
    }

    let mut sig_params: BTreeMap<String, String> = BTreeMap::new();
    sig_params.insert("api_key".to_string(), api_key.to_string());
    sig_params.insert("method".to_string(), "auth.getToken".to_string());
    let sig = compute_sig(&sig_params, api_secret);

    let url = format!(
        "{}?method=auth.getToken&api_key={}&api_sig={}&format=json",
        config.api_url(),
        api_key,
        sig
    );

    let resp = ureq::get(&url)
        .timeout(Duration::from_secs(15))
        .call()
        .map_err(|e| format!("HTTP error: {}", e))?;

    let text = resp.into_string().map_err(|e| e.to_string())?;
    let parsed: TokenResponse =
        serde_json::from_str(&text).map_err(|e| format!("Parse error: {}", e))?;

    if let Some(err) = parsed.error {
        return Err(format!(
            "API error {}: {}",
            err,
            parsed.message.unwrap_or_default()
        ));
    }

    parsed
        .token
        .ok_or_else(|| "No token in response".to_string())
}

/// Step 2: Bangun URL browser untuk user approve di website Last.fm/Libre.fm.
pub fn build_browser_url(config: &ScrobbleConfig, token: &str) -> String {
    format!(
        "{}?api_key={}&token={}",
        config.browser_auth_base_url(),
        config.effective_api_key(),
        token,
    )
}

/// Step 3: Poll `auth.getSession` setiap interval sampai user approve atau timeout.
/// Last.fm error 14 = token belum di-approve, lanjut polling.
/// Kembalikan (session_key, username).
pub fn poll_for_session(
    config: &ScrobbleConfig,
    token: &str,
    timeout_secs: u64,
    poll_interval_secs: u64,
) -> Result<(String, String), String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);

    while std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_secs(poll_interval_secs));

        match get_session_from_token(config, token) {
            Ok(result) => return Ok(result),
            Err(ref e) if e.contains("API error 14") => {
                // "This token has not been authorized" — user belum klik Allow
                continue;
            }
            Err(e) => return Err(e),
        }
    }

    Err(format!(
        "Timeout: token tidak di-approve dalam {} menit. Coba lagi dengan `morp --lastfm-auth`.",
        timeout_secs / 60
    ))
}

/// Step 4: Tukar token yang sudah diapprove dengan session key.
pub fn get_session_from_token(config: &ScrobbleConfig, token: &str) -> Result<(String, String), String> {
    let api_key = config.effective_api_key();
    let api_secret = config.effective_api_secret();

    let mut sig_params: BTreeMap<String, String> = BTreeMap::new();
    sig_params.insert("api_key".to_string(), api_key.to_string());
    sig_params.insert("method".to_string(), "auth.getSession".to_string());
    sig_params.insert("token".to_string(), token.to_string());
    let sig = compute_sig(&sig_params, api_secret);

    let url = format!(
        "{}?method=auth.getSession&api_key={}&token={}&api_sig={}&format=json",
        config.api_url(),
        api_key,
        token,
        sig
    );

    let resp = ureq::get(&url)
        .timeout(Duration::from_secs(15))
        .call()
        .map_err(|e| format!("HTTP error: {}", e))?;

    let text = resp.into_string().map_err(|e| e.to_string())?;
    let parsed: LastfmAuthResponse =
        serde_json::from_str(&text).map_err(|e| format!("Parse error: {}", e))?;

    if let Some(err) = parsed.error {
        return Err(format!(
            "API error {}: {}",
            err,
            parsed.message.unwrap_or_default()
        ));
    }

    let session = parsed
        .session
        .ok_or_else(|| "No session in response".to_string())?;
    let username = session.name.unwrap_or_default();
    Ok((session.key, username))
}

pub struct Scrobbler {
    config: ScrobbleConfig,
}

impl Scrobbler {
    pub fn new(config: ScrobbleConfig) -> Self {
        Self { config }
    }

    pub fn now_playing(&self, artist: &str, title: &str, album: &str, duration: Option<u64>) {
        if !self.config.is_ready() || artist.is_empty() || title.is_empty() {
            return;
        }

        let api_key = self.config.effective_api_key().to_string();
        let api_secret = self.config.effective_api_secret().to_string();

        let mut params: BTreeMap<String, String> = BTreeMap::new();
        params.insert("api_key".to_string(), api_key);
        params.insert("artist".to_string(), artist.to_string());
        params.insert("method".to_string(), "track.updateNowPlaying".to_string());
        params.insert("sk".to_string(), self.config.session_key.clone());
        params.insert("track".to_string(), title.to_string());
        if !album.is_empty() {
            params.insert("album".to_string(), album.to_string());
        }
        let duration_str = duration.map(|d| d.to_string());
        if let Some(ref ds) = duration_str {
            params.insert("duration".to_string(), ds.clone());
        }

        let sig = compute_sig(&params, &api_secret);
        params.insert("api_sig".to_string(), sig);
        params.insert("format".to_string(), "json".to_string());

        let body: Vec<(&str, &str)> = params
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        let _ = ureq::post(self.config.api_url())
            .timeout(Duration::from_secs(10))
            .send_form(&body);
    }

    pub fn scrobble(
        &self,
        artist: &str,
        title: &str,
        album: &str,
        duration: Option<u64>,
        timestamp: u64,
    ) {
        if !self.config.is_ready() || artist.is_empty() || title.is_empty() {
            return;
        }

        let api_key = self.config.effective_api_key().to_string();
        let api_secret = self.config.effective_api_secret().to_string();

        let ts_str = timestamp.to_string();
        let mut params: BTreeMap<String, String> = BTreeMap::new();
        params.insert("api_key".to_string(), api_key);
        params.insert("artist".to_string(), artist.to_string());
        params.insert("method".to_string(), "track.scrobble".to_string());
        params.insert("sk".to_string(), self.config.session_key.clone());
        params.insert("timestamp".to_string(), ts_str);
        params.insert("track".to_string(), title.to_string());
        if !album.is_empty() {
            params.insert("album".to_string(), album.to_string());
        }
        let duration_str = duration.map(|d| d.to_string());
        if let Some(ref ds) = duration_str {
            params.insert("duration".to_string(), ds.clone());
        }

        let sig = compute_sig(&params, &api_secret);
        params.insert("api_sig".to_string(), sig);
        params.insert("format".to_string(), "json".to_string());

        let body: Vec<(&str, &str)> = params
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        let _ = ureq::post(self.config.api_url())
            .timeout(Duration::from_secs(10))
            .send_form(&body);
    }
}
