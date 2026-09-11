//! The remote side: the review published to the worker so the reviewer
//! can decide from a phone, and the login that makes it theirs. The
//! configuration lives in ~/.config/commit-review/remote.json.

use std::path::PathBuf;
use std::time::Duration;

use crate::state::FileReview;

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct Config {
    /// Origin of the worker, e.g. https://commit-review.example.workers.dev
    pub url: String,
    /// Machine token the worker issued at login.
    pub token: String,
    /// GitHub login it belongs to.
    pub login: String,
}

/// A review the worker holds.
pub struct Published {
    pub id: String,
    pub url: String,
}

/// What the phone decided.
#[derive(serde::Deserialize)]
pub struct Decision {
    pub accept: bool,
    #[serde(default)]
    pub notes: String,
    pub reviews: Option<Vec<FileReview>>,
}

/// The worker parks a wait request this long before answering 204; the
/// client allows a little more.
const WAIT: Duration = Duration::from_secs(40);

fn config_path() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or("HOME is not set")?;
    Ok(PathBuf::from(home).join(".config").join("commit-review").join("remote.json"))
}

/// The login, if `auth login` was run on this machine.
pub fn load() -> Option<Config> {
    let text = std::fs::read_to_string(config_path().ok()?).ok()?;
    serde_json::from_str(&text).ok()
}

fn save(config: &Config) -> Result<(), String> {
    let path = config_path()?;
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

fn client(timeout: Duration) -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(timeout)
        .user_agent("commit-review")
        .build()
        .expect("http client")
}

fn hostname() -> String {
    let mut buf = [0u8; 256];
    // SAFETY: gethostname writes at most `len` bytes into a valid buffer.
    let ok = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) } == 0;
    let end = buf.iter().position(|&b| b == 0).unwrap_or(0);
    let name = String::from_utf8_lossy(&buf[..end]).to_string();
    if ok && !name.is_empty() { name } else { "this machine".to_string() }
}

/// GitHub's device flow through the worker: a code to type on github.com,
/// then a machine token saved for the hook.
pub fn login(url: &str) -> Result<(), String> {
    #[derive(serde::Deserialize)]
    struct Start {
        device_code: String,
        user_code: String,
        verification_uri: String,
        interval: u64,
        expires_in: u64,
    }
    #[derive(serde::Deserialize)]
    struct Done {
        token: String,
        login: String,
    }
    let url = url.trim_end_matches('/').to_string();
    let http = client(Duration::from_secs(30));
    let start: Start = http
        .post(format!("{url}/auth/start"))
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("cannot start the login: {e}"))?
        .json()
        .map_err(|e| format!("unexpected answer from the worker: {e}"))?;
    println!("Open {} and enter the code {}", start.verification_uri, start.user_code);
    let deadline = std::time::Instant::now() + Duration::from_secs(start.expires_in);
    let name = hostname();
    while std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_secs(start.interval + 1));
        let res = http
            .post(format!("{url}/auth/poll"))
            .json(&serde_json::json!({ "device_code": start.device_code, "kind": "machine", "name": name }))
            .send()
            .map_err(|e| format!("cannot reach the worker: {e}"))?;
        match res.status().as_u16() {
            202 => continue,
            200 => {
                let done: Done = res.json().map_err(|e| e.to_string())?;
                save(&Config { url, token: done.token, login: done.login.clone() })?;
                println!("Logged in as {} on {}", done.login, name);
                return Ok(());
            }
            _ => return Err(format!("login refused: {}", res.text().unwrap_or_default())),
        }
    }
    Err("the code expired before it was entered".to_string())
}

/// Sends the review; the answer names it.
pub fn publish(config: &Config, doc: &serde_json::Value) -> Result<Published, String> {
    #[derive(serde::Deserialize)]
    struct Created {
        id: String,
    }
    let created: Created = client(Duration::from_secs(60))
        .post(format!("{}/api/reviews", config.url))
        .bearer_auth(&config.token)
        .json(doc)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("publish failed: {e}"))?
        .json()
        .map_err(|e| e.to_string())?;
    Ok(Published { url: format!("{}/r/{}", config.url, created.id), id: created.id })
}

/// Blocks until the phone decides. 204 means nothing yet: ask again.
/// Gone (410) means the review was removed, so no decision will come.
pub fn wait(config: &Config, id: &str) -> Result<Decision, String> {
    let http = client(WAIT);
    loop {
        let res = http
            .get(format!("{}/api/reviews/{id}/wait", config.url))
            .bearer_auth(&config.token)
            .send()
            .map_err(|e| format!("wait failed: {e}"))?;
        match res.status().as_u16() {
            204 => continue,
            200 => return res.json().map_err(|e| e.to_string()),
            410 => return Err("review withdrawn".to_string()),
            code => return Err(format!("wait failed: HTTP {code}")),
        }
    }
}

/// The decision was made here: the phone has nothing left to decide.
pub fn withdraw(config: &Config, id: &str) {
    let _ = client(Duration::from_secs(5))
        .delete(format!("{}/api/reviews/{id}", config.url))
        .bearer_auth(&config.token)
        .send();
}

/// Forgets the login; the worker keeps its token until it expires.
pub fn logout() -> Result<(), String> {
    let path = config_path()?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}
