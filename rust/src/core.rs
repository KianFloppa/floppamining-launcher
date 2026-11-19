//! Everything the launcher does besides drawing: settings, folders, downloads, installing, the game's login file,
//! shortcuts / autostart / notifications. Same files and folders as the Godot launcher (1.x-2.5), so the Rust
//! launcher (3.0) takes over an existing install without anything being downloaded or set up again.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const LAUNCHER_VERSION: &str = "3.1.1";
pub const APP_NAME: &str = "Floppa Launcher";
pub const SITE: &str = "https://floppamining.com";
pub const API: &str = "https://api.floppamining.com";
pub const LOCK_PORT: u16 = 47731; // Floppa Mining holds this local port while it runs
pub const SELF_PORT: u16 = 47732; // the launcher holds this one: one launcher at a time; the game sends "update" here
pub const IS_LINUX: bool = cfg!(target_os = "linux");

pub struct Game {
    pub id: &'static str,
    pub title: &'static str,
    pub tag: &'static str,
    pub dir: &'static str,
    pub manifest: &'static str,
    pub news: &'static str,
    pub exe: &'static str,
    pub site: &'static str,
    pub notes: &'static str,   // 3.1.1: the "Patch notes" button (a game without patch note pages links its download page)
}

pub const GAMES: [Game; 3] = [
    Game { id: "mining", title: "Floppa Mining", tag: "FREE TO PLAY", dir: "game", manifest: "/launcher/latest.json",
        news: "/launcher/news.json", exe: "FloppaMining.exe", site: "https://floppamining.com",
        notes: "https://floppamining.com/updates/" },
    Game { id: "felony", title: "Floppa Felony", tag: "PUBLIC TEST", dir: "felony", manifest: "/launcher/felony.json",
        news: "/launcher/felony_news.json", exe: "FloppaFelony.exe", site: "https://felony.floppaland.com",
        notes: "https://felony.floppaland.com/updates/" },
    // 3.1 (2026-11-19): Floppa Mystery, public test (its own manifest + news on floppamining.com, like Felony)
    Game { id: "mystery", title: "Floppa Mystery", tag: "PUBLIC TEST", dir: "mystery", manifest: "/launcher/mystery.json",
        news: "/launcher/mystery_news.json", exe: "FloppaMystery.exe", site: "https://floppamining.com/download",
        notes: "https://floppamining.com/download/" },
];

pub fn game(id: &str) -> &'static Game {
    GAMES.iter().find(|g| g.id == id).unwrap_or(&GAMES[0])
}

pub fn edition() -> &'static str {
    if IS_LINUX { "Linux" } else { "Windows" }
}

// ---------------------------------------------------------------- folders
/// The launcher's own folder (Godot custom user dir "FloppaMining"): %APPDATA%\FloppaMining, ~/.local/share/FloppaMining
pub fn data_dir() -> PathBuf {
    base_data().join("FloppaMining")
}

fn base_data() -> PathBuf {
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
    } else {
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).filter(|p| p.is_absolute())
            .unwrap_or_else(|| home().join(".local/share"))
    }
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_default()
}

pub fn game_dir(id: &str) -> PathBuf {
    data_dir().join(game(id).dir)
}

/// Floppa Mining's own folder (its login file): %APPDATA%\Godot\app_userdata\Floppa Mining
pub fn mining_user_dir() -> PathBuf {
    base_data().join(if IS_LINUX { "godot" } else { "Godot" }).join("app_userdata/Floppa Mining")
}

// ---------------------------------------------------------------- Godot ConfigFile (launcher.cfg, the game's account.cfg)
/// The [section] key=value files Godot writes. Values stay as Godot text (true, 12, "text"), so keys this launcher
/// doesn't know are kept as they are.
#[derive(Default, Clone)]
pub struct Cfg {
    pub path: PathBuf,
    pub data: BTreeMap<String, BTreeMap<String, String>>,
}

impl Cfg {
    pub fn load(path: PathBuf) -> Cfg {
        let mut c = Cfg { path, data: BTreeMap::new() };
        let Ok(text) = fs::read_to_string(&c.path) else { return c };
        let mut sec = String::new();
        for line in text.lines() {
            let l = line.trim();
            if l.is_empty() || l.starts_with(';') {
                continue;
            }
            if l.starts_with('[') && l.ends_with(']') {
                sec = l[1..l.len() - 1].to_string();
                continue;
            }
            if let Some((k, v)) = l.split_once('=') {
                c.data.entry(sec.clone()).or_default().insert(k.trim().to_string(), v.trim().to_string());
            }
        }
        c
    }

    pub fn save(&self) {
        let mut out = String::new();
        for (sec, kv) in &self.data {
            out += &format!("[{sec}]\n\n");
            for (k, v) in kv {
                out += &format!("{k}={v}\n");
            }
            out += "\n";
        }
        if let Some(p) = self.path.parent() {
            let _ = fs::create_dir_all(p);
        }
        let _ = fs::write(&self.path, out);
    }

    fn raw(&self, sec: &str, key: &str) -> Option<&String> {
        self.data.get(sec).and_then(|m| m.get(key))
    }

    pub fn get_bool(&self, sec: &str, key: &str, def: bool) -> bool {
        match self.raw(sec, key).map(|s| s.as_str()) {
            Some("true") => true,
            Some("false") => false,
            _ => def,
        }
    }

    pub fn get_str(&self, sec: &str, key: &str, def: &str) -> String {
        match self.raw(sec, key) {
            Some(v) if v.starts_with('"') && v.ends_with('"') && v.len() >= 2 => unescape(&v[1..v.len() - 1]),
            Some(v) => v.clone(),
            None => def.to_string(),
        }
    }

    pub fn set_bool(&mut self, sec: &str, key: &str, v: bool) {
        self.data.entry(sec.into()).or_default().insert(key.into(), v.to_string());
    }

    pub fn set_str(&mut self, sec: &str, key: &str, v: &str) {
        self.data.entry(sec.into()).or_default().insert(key.into(), format!("\"{}\"", escape(v)));
    }
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}

fn unescape(s: &str) -> String {
    let mut o = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n') => o.push('\n'),
                Some('t') => o.push('\t'),
                Some(x) => o.push(x),
                None => {}
            }
        } else {
            o.push(c);
        }
    }
    o
}

// ---------------------------------------------------------------- versions
/// "1.6.11" -> "Open Beta 1.6.11"; old prototype numbers (15.x, Felony 116) -> "Prototype ..."
pub fn vname(v: &str) -> String {
    let major: i64 = v.split('.').next().and_then(|x| x.parse().ok()).unwrap_or(0);
    format!("{}{v}", if major >= 10 { "Prototype " } else { "Open Beta " })
}

/// 3.1: the version's name for a game: Floppa Mystery's builds are "Test 0.1.0" (its patch notes use the same words).
pub fn vname_for(game_id: &str, v: &str) -> String {
    if game_id == "mystery" { format!("Test {v}") } else { vname(v) }
}

/// 3.0.4: a release date ("2026-11-18") the short way: "today", "yesterday", "3 days ago", else "18 Nov 2026".
pub fn short_date(iso: &str) -> String {
    let p: Vec<i64> = iso.split('-').map(|x| x.parse().unwrap_or(0)).collect();
    if p.len() != 3 || p[0] < 2000 { return iso.to_string(); }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let today = (now + 3600) / 86400;   // the players are mostly in Europe: close enough for "today"
    let ago = today - days_from_civil(p[0], p[1], p[2]);
    const M: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    match ago {
        0 => "today".into(),
        1 => "yesterday".into(),
        2..=6 => format!("{ago} days ago"),
        _ => format!("{} {} {}", p[2], M[((p[1] - 1).clamp(0, 11)) as usize], p[0]),
    }
}

/// Days since 1970-01-01 (Howard Hinnant's civil date algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn newer(a: &str, b: &str) -> bool {
    let pa: Vec<i64> = a.split('.').map(|x| x.parse().unwrap_or(0)).collect();
    let pb: Vec<i64> = b.split('.').map(|x| x.parse().unwrap_or(0)).collect();
    for i in 0..pa.len().max(pb.len()) {
        let (x, y) = (pa.get(i).copied().unwrap_or(0), pb.get(i).copied().unwrap_or(0));
        if x != y {
            return x > y;
        }
    }
    false
}

pub fn mb(bytes: u64) -> String {
    if bytes > 0 { format!("{:.0} MB", bytes as f64 / 1048576.0) } else { "size unknown".into() }
}

// ---------------------------------------------------------------- network
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .http_status_as_error(false)
        .user_agent(format!("FloppaLauncher/{LAUNCHER_VERSION}"))
        .build()
        .into()
}

pub fn get_json(url: &str) -> Option<serde_json::Value> {
    let sep = if url.contains('?') { '&' } else { '?' };
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let mut r = agent().get(&format!("{url}{sep}t={t}")).call().ok()?;
    if r.status() != 200 {
        return None;
    }
    let s = r.body_mut().with_config().limit(4 << 20).read_to_string().ok()?;
    serde_json::from_str(&s).ok()
}

pub fn get_bytes(url: &str, limit: u64) -> Option<Vec<u8>> {
    let mut r = agent().get(url).call().ok()?;
    if r.status() != 200 {
        return None;
    }
    r.body_mut().with_config().limit(limit).read_to_vec().ok()
}

/// Game server call: (status code, json body); code 0 = no connection.
pub fn api(method: &str, url: &str, body: Option<serde_json::Value>, login: Option<(&str, &str)>) -> (u16, serde_json::Value) {
    let url = url.to_string();
    let a = agent();
    let auth = login.filter(|(i, t)| !i.is_empty() && !t.is_empty()).map(|(i, t)| format!("Bearer {i}.{t}"));
    let res = if method == "GET" {
        let mut q = a.get(&url);
        if let Some(h) = &auth {
            q = q.header("authorization", h);
        }
        q.call()
    } else {
        let mut q = a.post(&url).header("content-type", "application/json");
        if let Some(h) = &auth {
            q = q.header("authorization", h);
        }
        q.send(body.unwrap_or(serde_json::json!({})).to_string())
    };
    match res {
        Ok(mut r) => {
            let code = r.status().as_u16();
            let s = r.body_mut().read_to_string().unwrap_or_default();
            (code, serde_json::from_str(&s).unwrap_or(serde_json::json!({})))
        }
        Err(_) => (0, serde_json::json!({})),
    }
}

/// Download to a file, reporting (bytes so far, total) now and then. No time limit (big files on slow internet).
pub fn download(url: &str, to: &Path, progress: &dyn Fn(u64, u64)) -> Result<(), String> {
    let a: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .http_status_as_error(false)
        .user_agent(format!("FloppaLauncher/{LAUNCHER_VERSION}"))
        .build()
        .into();
    let mut r = a.get(url).call().map_err(|e| format!("The download couldn't start ({e})."))?;
    if r.status() != 200 {
        return Err(format!("The download failed (HTTP {}). Check your internet and try again.", r.status()));
    }
    let total: u64 = r.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok()).unwrap_or(0);
    if let Some(p) = to.parent() {
        let _ = fs::create_dir_all(p);
    }
    let mut f = fs::File::create(to).map_err(|_| "Couldn't save the download (is there enough space?).".to_string())?;
    let mut rd = r.body_mut().with_config().limit(u64::MAX).reader();
    let mut buf = vec![0u8; 256 * 1024];
    let mut got = 0u64;
    loop {
        let n = rd.read(&mut buf).map_err(|_| "The download stopped. Check your internet and try again.".to_string())?;
        if n == 0 {
            break;
        }
        f.write_all(&buf[..n]).map_err(|_| "Couldn't save the download (is there enough space?).".to_string())?;
        got += n as u64;
        progress(got, total);
    }
    Ok(())
}

pub fn sha256_file(p: &Path) -> String {
    let Ok(mut f) = fs::File::open(p) else { return String::new() };
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        match f.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => h.update(&buf[..n]),
            Err(_) => return String::new(),
        }
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------- install
pub fn extract(zip_path: &Path, to: &Path) -> Result<(), String> {
    let _ = fs::remove_dir_all(to);
    fs::create_dir_all(to).map_err(|_| "Couldn't write the game files (is there enough space?).".to_string())?;
    let f = fs::File::open(zip_path).map_err(|_| "The download couldn't be opened.".to_string())?;
    let mut z = zip::ZipArchive::new(f).map_err(|_| "The download couldn't be opened.".to_string())?;
    for i in 0..z.len() {
        let mut e = z.by_index(i).map_err(|_| "The download is damaged.".to_string())?;
        let Some(rel) = e.enclosed_name() else { continue }; // no absolute paths or ".."
        let out = to.join(rel);
        if e.is_dir() {
            let _ = fs::create_dir_all(&out);
            continue;
        }
        if let Some(p) = out.parent() {
            let _ = fs::create_dir_all(p);
        }
        let mut w = fs::File::create(&out).map_err(|_| "Couldn't write the game files (is there enough space?).".to_string())?;
        std::io::copy(&mut e, &mut w).map_err(|_| "Couldn't write the game files (is there enough space?).".to_string())?;
    }
    Ok(())
}

pub fn installed_version(id: &str, exe: &str) -> String {
    let d = game_dir(id);
    if !d.join(exe).exists() {
        return String::new();
    }
    fs::read_to_string(d.join("version.txt")).map(|s| s.trim().to_string()).unwrap_or_default()
}

/// Download, check and install a game build into its folder. Runs on a worker thread.
pub fn install(id: &str, url: &str, sha: &str, version: &str, exe: &str, progress: &dyn Fn(u64, u64)) -> Result<(), String> {
    let zip = data_dir().join("download.zip");
    let _ = fs::remove_file(&zip);
    download(url, &zip, progress)?;
    if !sha.is_empty() && sha256_file(&zip) != sha.to_lowercase() {
        let _ = fs::remove_file(&zip);
        return Err("The download was damaged. Click to try again.".into());
    }
    let dir = game_dir(id);
    let tmp = dir.with_file_name(format!("{}_new", game(id).dir));
    let r = extract(&zip, &tmp);
    let _ = fs::remove_file(&zip);
    if let Err(e) = r {
        let _ = fs::remove_dir_all(&tmp);
        return Err(e);
    }
    let _ = fs::remove_dir_all(&dir);
    fs::rename(&tmp, &dir).map_err(|_| "Couldn't finish installing. Is the game still open?".to_string())?;
    set_runnable(&dir.join(exe));
    fs::write(dir.join("version.txt"), version).map_err(|_| "Couldn't finish installing.".to_string())?;
    Ok(())
}

#[cfg(unix)]
pub fn set_runnable(p: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(m) = fs::metadata(p) {
        let mut perm = m.permissions();
        perm.set_mode(perm.mode() | 0o755);
        let _ = fs::set_permissions(p, perm);
    }
}
#[cfg(not(unix))]
pub fn set_runnable(_p: &Path) {}

// ---------------------------------------------------------------- self-update
/// Windows and Linux let a running program be renamed: rename ourselves to .old, put the new exe in our place,
/// start it and quit. The new launcher deletes the .old file when it starts.
pub fn self_update(url: &str, sha: &str) -> Result<(), String> {
    let zip = data_dir().join("launcher_update.zip");
    download(url, &zip, &|_, _| {})?;
    if !sha.is_empty() && sha256_file(&zip) != sha.to_lowercase() {
        let _ = fs::remove_file(&zip);
        return Err("damaged".into());
    }
    let f = fs::File::open(&zip).map_err(|e| e.to_string())?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
    let want = if IS_LINUX { ".x86_64" } else { ".exe" };
    let mut data = Vec::new();
    for i in 0..z.len() {
        let mut e = z.by_index(i).map_err(|e| e.to_string())?;
        if e.name().map(|n| n.to_lowercase().ends_with(want)).unwrap_or(false) {
            e.read_to_end(&mut data).map_err(|e| e.to_string())?;
        }
    }
    let _ = fs::remove_file(&zip);
    if data.len() < 1_000_000 {
        return Err("no launcher in the zip".into());
    }
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let old = PathBuf::from(format!("{}.old", me.display()));
    let _ = fs::remove_file(&old);
    fs::rename(&me, &old).map_err(|e| e.to_string())?;
    if fs::write(&me, &data).is_err() {
        let _ = fs::rename(&old, &me);
        return Err("couldn't write".into());
    }
    set_runnable(&me);
    let _ = std::process::Command::new(&me).spawn();
    std::process::exit(0);
}

pub fn remove_old_exe() {
    if let Ok(me) = std::env::current_exe() {
        let _ = fs::remove_file(format!("{}.old", me.display()));
    }
}

// ---------------------------------------------------------------- the game
/// Floppa Mining listens on 127.0.0.1:LOCK_PORT while it runs: if we can't take the port, it is open.
pub fn mining_running() -> bool {
    std::net::TcpListener::bind(("127.0.0.1", LOCK_PORT)).is_err()
}

/// The game finds the launcher through this file when a new version is out (it starts it with --update).
pub fn write_path_file() {
    let d = mining_user_dir();
    let _ = fs::create_dir_all(&d);
    if let Ok(me) = std::env::current_exe() {
        let _ = fs::write(d.join("launcher_path.txt"), me.display().to_string());
    }
}

pub fn open_url(u: &str) {
    let _ = open::that_detached(u);
}

pub fn open_folder(p: &Path) {
    let _ = fs::create_dir_all(p);
    let _ = open::that_detached(p);
}

// ---------------------------------------------------------------- shortcuts, autostart (no PowerShell, no extra programs)
pub fn apply_system(cfg: &Cfg) {
    let me = match std::env::current_exe() {
        Ok(m) => m,
        Err(_) => return,
    };
    #[cfg(windows)]
    {
        // Desktop + Start menu shortcuts, remade every start so they follow the launcher if it was moved
        let desktop = std::env::var_os("USERPROFILE").map(|u| PathBuf::from(u).join("Desktop"));
        let programs = std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Microsoft/Windows/Start Menu/Programs"));
        for (key, dir) in [("desktop", desktop), ("startmenu", programs)] {
            let Some(dir) = dir else { continue };
            let lnk = dir.join("Floppa Mining.lnk");
            if cfg.get_bool("win", key, true) {
                if let Ok(mut l) = mslnk::ShellLink::new(&me) {
                    l.set_working_dir(me.parent().map(|p| p.display().to_string()));
                    l.set_icon_location(Some(me.display().to_string()));
                    l.set_name(Some(APP_NAME.to_string()));
                    let _ = l.create_lnk(&lnk);
                }
            } else {
                let _ = fs::remove_file(&lnk);
            }
        }
        // Start with Windows: only when the player turned it on (Settings)
        use winreg::enums::*;
        let hk = winreg::RegKey::predef(HKEY_CURRENT_USER);
        if let Ok((run, _)) = hk.create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run") {
            if cfg.get_bool("win", "autostart", false) {
                let _ = run.set_value("FloppaMiningLauncher", &format!("\"{}\" --tray", me.display()));
            } else {
                let _ = run.delete_value("FloppaMiningLauncher");
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        let home = home();
        let icon = data_dir().join("launcher_icon.png");
        let _ = fs::create_dir_all(data_dir());
        let _ = fs::write(&icon, include_bytes!("../assets/tray.png"));
        let desk = std::process::Command::new("xdg-user-dir").arg("DESKTOP").output().ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).filter(|s| !s.is_empty())
            .map(PathBuf::from).unwrap_or_else(|| home.join("Desktop"));
        let files = [
            (desk.join("floppa-launcher.desktop"), "desktop", true, ""),
            (home.join(".local/share/applications/floppa-launcher.desktop"), "startmenu", true, ""),
            (home.join(".config/autostart/floppa-launcher.desktop"), "autostart", false, " --tray"),
        ];
        for (path, key, def, extra) in files {
            if cfg.get_bool("win", key, def) {
                if let Some(p) = path.parent() {
                    let _ = fs::create_dir_all(p);
                }
                let entry = format!("[Desktop Entry]\nType=Application\nName=Floppa Launcher\nComment=Install, update and play the Floppa games\nExec=\"{}\"{}\nIcon={}\nTerminal=false\nCategories=Game;\nStartupWMClass=Floppa Launcher\n", me.display(), extra, icon.display());
                let _ = fs::write(&path, entry);
                set_runnable(&path);
            } else {
                let _ = fs::remove_file(&path);
            }
        }
    }
}

// ---------------------------------------------------------------- notifications
pub fn toast(title: &str, body: &str) {
    let mut n = notify_rust::Notification::new();
    n.summary(title).body(body);
    #[cfg(target_os = "linux")]
    n.appname(APP_NAME);
    let _ = n.show();
}

#[cfg(test)]
mod date_tests {
    use super::*;
    #[test]
    fn civil() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2026, 11, 18), 20775);
        assert_eq!(short_date("2020-03-05"), "5 Mar 2020");
        assert_eq!(short_date("bad"), "bad");
    }
}
