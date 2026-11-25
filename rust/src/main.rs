//! Floppa Launcher 3.0 (Rust): installs, updates and starts the Floppa games (Floppa Mining, Floppa Felony).
//! Same screen, buttons, settings and files as the Godot launcher 2.5, in a few MB instead of ~100 MB.
//! The website is the source of truth: <site>/launcher/latest.json (Mining + the launcher version), felony.json, news.
//! Test options: --site=http://127.0.0.1:8124  --api=...  --no-self-update  --shot=/tmp/x.png (screenshot, then quit)
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod core;

use crate::core::*;
use eframe::egui::{self, Align2, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const GOLD: Color32 = Color32::from_rgb(0xff, 0xd1, 0x66);
const INK: Color32 = Color32::from_rgb(0x1b, 0x10, 0x06);
const PANEL: Color32 = Color32::from_rgba_premultiplied(13, 13, 18, 219);

static SITE_URL: OnceLock<String> = OnceLock::new();
static API_URL: OnceLock<String> = OnceLock::new();
fn site() -> &'static str { SITE_URL.get().map(|s| s.as_str()).unwrap_or(SITE) }
fn api_url() -> &'static str { API_URL.get().map(|s| s.as_str()).unwrap_or(API) }

fn arg(a: &str) -> bool { std::env::args().any(|x| x == a) }
fn arg_val(p: &str) -> Option<String> { std::env::args().find_map(|x| x.strip_prefix(p).map(|s| s.to_string())) }

/// News / image links: full addresses as they are, paths on the site.
/// A news picture as a small thumbnail, kept on disk (cache\news) so it is downloaded only once.
/// Each update has its own picture address, so a cached file never goes stale.
fn news_thumb(url: &str) -> Option<image::RgbaImage> {
    use sha2::{Digest, Sha256};
    let dir = data_dir().join("cache").join("news");
    let name: String = Sha256::digest(url.as_bytes()).iter().take(12).map(|b| format!("{b:02x}")).collect();
    let file = dir.join(format!("{name}.png"));
    if let Ok(i) = image::open(&file) { return Some(i.to_rgba8()); }
    let bytes = get_bytes(url, 16 << 20)?;
    let img = image::load_from_memory(&bytes).ok()?.thumbnail(668, 208).to_rgba8();
    let _ = std::fs::create_dir_all(&dir);
    let _ = img.save(&file);
    // keep the folder small: only the 30 newest pictures
    if let Ok(rd) = std::fs::read_dir(&dir) {
        let mut v: Vec<_> = rd.flatten().filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path()))).collect();
        if v.len() > 30 { v.sort(); for (_, p) in &v[..v.len() - 30] { let _ = std::fs::remove_file(p); } }
    }
    Some(img)
}

fn abs(u: &str) -> String { if u.starts_with("http") { u.to_string() } else { format!("{}{u}", site()) } }

enum Msg {
    ChatAsk,   // 3.4: FloppaChat's "Update now": it closes, then we update it and open it again
    Manifest { id: String, data: Option<Value>, play: bool },
    News { gen: u32, items: Vec<Value> },
    NewsImage { gen: u32, idx: usize, url: String, img: egui::ColorImage },
    Background { id: String, img: egui::ColorImage },
    Progress(u64, u64),
    Installed { id: String, result: Result<(), String> },
    SelfUpdateFailed,
    Api { kind: &'static str, code: u16, data: Value },
    Show,
    Update(String), // the game that asked (3.1)
    Tray(u8), // 0 open, 1 play, 2 check, 3 quit
    Tick,
    ChatManifest(Option<Value>),          // 3.2: FloppaChat's manifest (chat.json)
    ChatProgress(u64, u64),
    ChatInstalled { result: Result<(), String>, open: bool },
}

#[derive(PartialEq, Clone)]
enum Page { None, Login, Signup, Code(String), Account }

struct Card { title: String, date: String, v: String, url: String, tex: Option<egui::TextureHandle> }

static HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// Wake a hidden window from any thread (tray, local port). Elsewhere a no-op: there the viewport commands work.
fn wake_window() {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW};
        let h = HWND.load(std::sync::atomic::Ordering::Relaxed);
        if h != 0 {
            unsafe {
                ShowWindow(h as _, SW_SHOW);
                ShowWindow(h as _, SW_RESTORE);
                SetForegroundWindow(h as _);
            }
        }
    }
}

struct App {
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    ctx: egui::Context,
    cfg: Cfg,
    cur: String,
    manifest: Value,
    mstore: HashMap<String, Value>,
    installed: String,
    busy: bool,
    self_updating: bool,
    check_failed: bool,
    status: String,
    progress: Option<(u64, u64)>,
    speed: f64,
    last_prog: (Instant, u64),
    game_running: bool,
    child: Option<(String, std::process::Child)>,   // the Felony / Mystery process it started (no lock port)
    news: Vec<Card>,
    news_gen: u32,
    /// News pictures already shown this session, by address (no new download when switching tabs).
    img_cache: HashMap<String, egui::TextureHandle>,
    news_loaded: bool,
    bg: HashMap<&'static str, egui::TextureHandle>,
    recheck_at: Instant,
    running_check_at: Instant,
    settings_open: bool,
    page: Page,
    f_name: String,
    f_pw: String,
    f_pw2: String,
    f_msg: String,
    f_busy: bool,
    acc_id: String,
    acc_token: String,
    acc_name: String,
    self_update_ok: bool,
    shot: Option<String>,
    shot_at: Option<Instant>,
    hidden: bool,
    panel_h: f32,
    quit_at: Option<Instant>,
    here_at: Option<Instant>,    // 3.3: last "launcher open" beat (POST /v1/here) while the window shows; None = none / left
    chat: Value,                 // 3.2: FloppaChat's manifest for this system (null: not loaded / none for it)
    chat_installed: String,
    chat_busy: bool,
    chat_prog: Option<(u64, u64)>,
    chat_click: bool,
    chat_wait: Option<Instant>,  // 3.4: update FloppaChat as soon as it has closed (asked by FloppaChat, until then)
    chat_err: String,            // the last FloppaChat problem (shown on its button until the next try)
    #[cfg(windows)]
    _tray: Option<tray_icon::TrayIcon>,
}

static JUST_UPDATED: OnceLock<bool> = OnceLock::new();
fn just_updated() -> bool { *JUST_UPDATED.get().unwrap_or(&false) }

fn main() -> eframe::Result<()> {
    if let Some(s) = arg_val("--site=") { let _ = SITE_URL.set(s); }
    if let Some(s) = arg_val("--api=") { let _ = API_URL.set(s); }
    // one launcher at a time: if one runs already, ask it to show itself (or update) and quit
    // Right after an update (the old exe is still next to us as .old) the old launcher may not have let go of the
    // port yet: it started us and is quitting. Wait for it a little instead of thinking another launcher runs.
    let just_updated = std::env::current_exe().map(|m| std::path::PathBuf::from(format!("{}.old", m.display())).exists()).unwrap_or(false);
    let mut bound = std::net::TcpListener::bind(("127.0.0.1", SELF_PORT));
    if bound.is_err() && just_updated {
        for _ in 0..40 {
            std::thread::sleep(Duration::from_millis(150));
            bound = std::net::TcpListener::bind(("127.0.0.1", SELF_PORT));
            if bound.is_ok() { break; }
        }
    }
    let listener = match bound {
        Ok(l) => Some(l),
        Err(_) => {
            if let Ok(mut s) = std::net::TcpStream::connect_timeout(&([127, 0, 0, 1], SELF_PORT).into(), Duration::from_millis(800)) {
                let _ = s.write_all(if arg("--update") { b"update\n" } else { b"show\n" });
            }
            return Ok(());
        }
    };
    let _ = JUST_UPDATED.set(just_updated);
    remove_old_exe();
    let icon = image::load_from_memory(include_bytes!("../assets/icon.png")).ok().map(|i| {
        let i = i.to_rgba8();
        egui::IconData { width: i.width(), height: i.height(), rgba: i.into_raw() }
    });
    let mut vp = egui::ViewportBuilder::default()
        .with_inner_size([1100.0, 640.0])
        .with_resizable(false)
        .with_maximize_button(false)
        .with_title(format!("{APP_NAME} · {} edition", edition()))
        .with_app_id("Floppa Launcher");
    if let Some(i) = icon { vp = vp.with_icon(std::sync::Arc::new(i)); }
    if arg("--tray") { vp = vp.with_visible(false); }
    let opts = eframe::NativeOptions { viewport: vp, ..Default::default() };
    eframe::run_native(APP_NAME, opts, Box::new(move |cc| Ok(Box::new(App::new(cc, listener)))))
}

impl App {
    fn new(cc: &eframe::CreationContext, listener: Option<std::net::TcpListener>) -> App {
        let ctx = cc.egui_ctx.clone();
        #[cfg(windows)]
        {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(h) = cc.window_handle() {
                if let RawWindowHandle::Win32(w) = h.as_raw() { HWND.store(w.hwnd.get(), std::sync::atomic::Ordering::Relaxed); }
            }
        }
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert("sniglet".into(), std::sync::Arc::new(egui::FontData::from_static(include_bytes!("../assets/Sniglet-Regular.ttf"))));
        fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "sniglet".into());
        ctx.set_fonts(fonts);
        ctx.set_visuals(egui::Visuals::dark());
        let (tx, rx) = channel();
        // messages from the game or a second launcher: "show" or "update"
        if let Some(l) = listener {
            let (tx2, c2) = (tx.clone(), ctx.clone());
            std::thread::spawn(move || {
                for s in l.incoming().flatten() {
                    let mut s = s;
                    let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
                    let mut buf = [0u8; 64];
                    let n = s.read(&mut buf).unwrap_or(0);
                    let msg = String::from_utf8_lossy(&buf[..n]).to_string();
                    // "update" (Floppa Mining) or "update <game id>" (3.1: Floppa Mystery sends "update mystery")
                    let _ = tx2.send(if msg.trim() == "chat" { Msg::ChatAsk } else if let Some(rest) = msg.strip_prefix("update") {
                        let g = rest.trim();
                        Msg::Update(if GAMES.iter().any(|x| x.id == g) { g.to_string() } else { "mining".to_string() })
                    } else { Msg::Show });
                    wake_window();
                    c2.request_repaint();
                }
            });
        }
        // a heartbeat so update checks and "is the game running" happen while the window is hidden in the tray
        {
            let (tx2, c2) = (tx.clone(), ctx.clone());
            std::thread::spawn(move || loop {
                std::thread::sleep(Duration::from_millis(1500));
                if tx2.send(Msg::Tick).is_err() { break; }
                c2.request_repaint();
            });
        }
        let cfg = Cfg::load(data_dir().join("launcher.cfg"));
        let want = cfg.get_str("launcher", "game", "mining");
        let cur = if shown_games(cfg.get_bool("launcher", "hide_in_work", true)).iter().any(|g| g.id == want) { want } else { "mining".into() };
        let mut bg = HashMap::new();
        for (k, bytes) in [("mining", &include_bytes!("../assets/bg.jpg")[..]), ("felony", &include_bytes!("../assets/bg_felony.jpg")[..]),
            ("mystery", &include_bytes!("../assets/bg_mystery.jpg")[..])] {
            // 3.1.4: the site's main picture from the last time (cache\bg), else the picture built into the launcher
            let cached = image::open(data_dir().join("cache").join("bg").join(format!("{k}.png"))).ok();
            if let Some(i) = cached.map(|i| i.to_rgba8()) {
                let ci = egui::ColorImage::from_rgba_unmultiplied([i.width() as usize, i.height() as usize], &i);
                bg.insert(k, ctx.load_texture(k, ci, egui::TextureOptions::LINEAR));
                continue;
            }
            if let Ok(i) = image::load_from_memory(bytes) {
                let i = i.to_rgba8();
                let ci = egui::ColorImage::from_rgba_unmultiplied([i.width() as usize, i.height() as usize], &i);
                bg.insert(k, ctx.load_texture(k, ci, egui::TextureOptions::LINEAR));
            }
        }
        let mut app = App {
            tx, rx, ctx: ctx.clone(), cfg, cur, manifest: Value::Null, mstore: HashMap::new(), installed: String::new(),
            busy: false, self_updating: false, check_failed: false, status: "Checking for updates...".into(), progress: None,
            speed: 0.0, last_prog: (Instant::now(), 0), game_running: false, child: None, news: vec![], news_gen: 0, img_cache: HashMap::new(),
            news_loaded: false, bg, recheck_at: Instant::now() + Duration::from_secs(600), running_check_at: Instant::now(),
            settings_open: false, page: Page::None, f_name: String::new(), f_pw: String::new(), f_pw2: String::new(),
            f_msg: String::new(), f_busy: false, acc_id: String::new(), acc_token: String::new(), acc_name: String::new(),
            self_update_ok: !arg("--no-self-update") && !cfg!(debug_assertions), shot: arg_val("--shot="), shot_at: None,
            hidden: arg("--tray"), quit_at: None, here_at: None, panel_h: 420.0,
            chat: Value::Null, chat_installed: chat_installed(), chat_busy: false, chat_prog: None, chat_click: false, chat_wait: None, chat_err: String::new(),
            #[cfg(windows)]
            _tray: None,
        };
        app.installed = installed_version(&app.cur, app.exe_name());
        app.setup_tray();
        apply_system(&app.cfg);
        write_path_file();
        app.fetch_all();
        app.load_account();
        if arg("--update") { app.check_now(); }
        match arg_val("--open=").as_deref() {      // screenshots for tests
            Some("settings") => app.settings_open = true,
            Some("login") => app.page = Page::Login,
            Some("signup") => app.page = Page::Signup,
            Some("chat") => app.chat_wait = Some(Instant::now() + Duration::from_secs(15)),   // 3.4: as soon as FloppaChat is closed (3.2: at once)
            _ => {}
        }
        app
    }

    // ------------------------------------------------------------ tray
    // 3.0.2: a window hidden with Visible(false) gets no more frames on Windows, so it never handled the
    // "show" commands (tray click, menu, the game's "Update now"): the launcher looked dead. These threads now
    // show the window through Windows directly; its frames start again and update() handles the message.
    #[cfg(windows)]
    fn setup_tray(&mut self) {
        use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
        let img = image::load_from_memory(include_bytes!("../assets/tray.png")).map(|i| i.to_rgba8()).ok();
        let Some(img) = img else { return };
        let Ok(icon) = tray_icon::Icon::from_rgba(img.clone().into_raw(), img.width(), img.height()) else { return };
        let menu = Menu::new();
        let items = [MenuItem::new("Open", true, None), MenuItem::new("Play", true, None), MenuItem::new("Check for updates", true, None), MenuItem::new("Quit", true, None)];
        let _ = menu.append(&items[0]);
        let _ = menu.append(&items[1]);
        let _ = menu.append(&items[2]);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&items[3]);
        let ids: Vec<_> = items.iter().map(|i| i.id().clone()).collect();
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            if let Some(i) = ids.iter().position(|x| *x == e.id) {
                if i == 3 { std::process::exit(0); } // Quit works even while the window gets no frames
                let _ = tx.send(Msg::Tray(i as u8));
                wake_window();
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.request_repaint();
            }
        }));
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        tray_icon::TrayIconEvent::set_event_handler(Some(move |e: tray_icon::TrayIconEvent| {
            if let tray_icon::TrayIconEvent::Click { button: tray_icon::MouseButton::Left, .. } = e {
                let _ = tx.send(Msg::Show);
                wake_window();
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.request_repaint();
            }
        }));
        self._tray = tray_icon::TrayIconBuilder::new().with_icon(icon).with_tooltip(APP_NAME).with_menu(Box::new(menu)).build().ok();
    }

    #[cfg(target_os = "linux")]
    fn setup_tray(&mut self) {
        struct T { tx: Sender<Msg>, ctx: egui::Context }
        impl T {
            fn send(&self, m: Msg) {
                let _ = self.tx.send(m);
                self.ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                self.ctx.request_repaint();
            }
        }
        impl ksni::Tray for T {
            fn id(&self) -> String { "floppa-launcher".into() }
            fn title(&self) -> String { APP_NAME.into() }
            fn icon_pixmap(&self) -> Vec<ksni::Icon> {
                let Ok(i) = image::load_from_memory(include_bytes!("../assets/tray.png")) else { return vec![] };
                let i = i.to_rgba8();
                let mut data = i.clone().into_raw();
                for px in data.chunks_exact_mut(4) { px.rotate_right(1); } // RGBA -> ARGB
                vec![ksni::Icon { width: i.width() as i32, height: i.height() as i32, data }]
            }
            fn activate(&mut self, _x: i32, _y: i32) { self.send(Msg::Show); }
            fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
                use ksni::menu::StandardItem;
                let item = |label: &str, n: u8| -> ksni::MenuItem<Self> {
                    StandardItem { label: label.into(), activate: Box::new(move |t: &mut T| t.send(Msg::Tray(n))), ..Default::default() }.into()
                };
                vec![item("Open", 0), item("Play", 1), item("Check for updates", 2), ksni::MenuItem::Separator, item("Quit", 3)]
            }
        }
        let t = T { tx: self.tx.clone(), ctx: self.ctx.clone() };
        std::thread::spawn(move || {
            use ksni::blocking::TrayMethods;
            if let Ok(h) = t.spawn() { loop { std::thread::sleep(Duration::from_secs(3600)); let _ = &h; } }
        });
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    fn setup_tray(&mut self) {}

    fn show_window(&mut self) {
        self.hidden = false;
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    fn to_tray(&mut self) {
        self.hidden = true;
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }

    // ------------------------------------------------------------ manifests
    fn exe_name(&self) -> &str {
        let m = self.manifest.get("exe").and_then(|v| v.as_str());
        match m {
            Some(e) => e,
            None if self.cur == "mining" => {
                // Mining remembers its exe name from the last check (the Godot launcher did the same)
                if self.cfg.get_str("launcher", "exe", "").is_empty() { game("mining").exe } else { "FloppaMining.exe" }
            }
            None => game(&self.cur).exe,
        }
    }

    fn fetch(&self, id: &str, play: bool) {
        let (tx, ctx, id) = (self.tx.clone(), self.ctx.clone(), id.to_string());
        let url = format!("{}{}", site(), game(&id).manifest);
        std::thread::spawn(move || {
            let data = get_json(&url);
            let _ = tx.send(Msg::Manifest { id, data, play });
            ctx.request_repaint();
        });
    }

    fn fetch_all(&mut self) {
        for g in GAMES.iter() { self.fetch(g.id, false); }
        self.fetch_chat();
        self.fetch_news();
        self.fetch_backgrounds();
    }

    /// 3.1.4: every game's background is the main picture of its website (the hero). It is kept in cache\bg with the
    /// address and ETag it came from, and checked again with every update check: a new picture on the site (another
    /// file, or the same file changed) shows up in the launcher; no internet = the last one (or the built-in one).
    fn fetch_backgrounds(&self) {
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let dir = data_dir().join("cache").join("bg");
            let _ = std::fs::create_dir_all(&dir);
            for g in GAMES.iter() {
                let Some(html) = get_bytes(g.home, 4 << 20).map(|b| String::from_utf8_lossy(&b).into_owned()) else { continue };
                let Some(url) = hero_url(&html, g.home) else { continue };
                let meta = dir.join(format!("{}.txt", g.id));
                let old = std::fs::read_to_string(&meta).unwrap_or_default();
                let mut lines = old.lines();
                let (old_url, old_tag) = (lines.next().unwrap_or(""), lines.next().unwrap_or(""));
                let have = dir.join(format!("{}.png", g.id)).exists();
                let tag = if old_url == url && have { old_tag } else { "" };
                let Some((code, body, etag)) = get_if_changed(&url, tag, 24 << 20) else { continue };
                if code != 200 { continue; }      // 304: the picture we have is still the site's picture
                let Ok(img) = image::load_from_memory(&body) else { continue };
                let img = img.thumbnail(1280, 1280).to_rgba8();
                if img.save(dir.join(format!("{}.png", g.id))).is_ok() {
                    let _ = std::fs::write(&meta, format!("{url}\n{etag}\n"));
                }
                let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], &img);
                let _ = tx.send(Msg::Background { id: g.id.to_string(), img: ci });
                ctx.request_repaint();
            }
        });
    }

    fn fetch_news(&mut self) {
        self.news_gen += 1;
        self.news.clear();
        self.news_loaded = false;
        let (tx, ctx, gen) = (self.tx.clone(), self.ctx.clone(), self.news_gen);
        let url = abs(game(&self.cur).news);
        let known: std::collections::HashSet<String> = self.img_cache.keys().cloned().collect();
        std::thread::spawn(move || {
            let items = get_json(&url).and_then(|v| v.as_array().cloned()).unwrap_or_default();
            let items: Vec<Value> = items.into_iter().take(3).collect();
            let _ = tx.send(Msg::News { gen, items: items.clone() });
            ctx.request_repaint();
            for (idx, it) in items.iter().enumerate() {
                let Some(u) = it.get("image").and_then(|v| v.as_str()).filter(|u| !u.is_empty()) else { continue };
                let u = abs(u);
                if known.contains(&u) { continue; } // already in memory: shown from there
                let Some(img) = news_thumb(&u) else { continue };
                let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], &img);
                let _ = tx.send(Msg::NewsImage { gen, idx, url: u, img: ci });
                ctx.request_repaint();
            }
        });
    }

    /// Linux takes the manifest's "linux" block; no block = no Linux build of that version.
    fn for_platform(d: &Value) -> Option<Value> {
        if !IS_LINUX { return Some(d.clone()); }
        let lx = d.get("linux")?.as_object()?;
        if !lx.contains_key("url") { return None; }
        let mut m = d.clone();
        for (k, v) in lx { m[k] = v.clone(); }
        Some(m)
    }

    fn on_manifest_for(&mut self, id: String, data: Option<Value>, play: bool) {
        let valid = data.as_ref().map(|d| d.get("game").is_some() && d.get("url").is_some()).unwrap_or(false);
        if id == "mining" && valid {
            let d = data.clone().unwrap();
            self.mstore.insert("mining".into(), d.clone());
            let lv = d.get("launcher").and_then(|v| v.as_str()).unwrap_or("0").to_string();
            // never twice in a row: right after an update (the .old exe still lies next to us) no new one, so a wrong
            // version number can't make a restart loop again (3.3.0/3.3.1)
            if newer(&lv, LAUNCHER_VERSION) && self.self_update_ok && !self.busy && !self.self_updating && !just_updated() {
                self.self_update(&d);
                return;
            }
        }
        if id != self.cur {
            if valid { self.mstore.insert(id, data.unwrap()); }
            return;
        }
        if play {
            self.busy = false;
            let Some(d) = data.filter(|_| valid).and_then(|d| Self::for_platform(&d)) else {
                self.status = "Couldn't check for updates. Check your internet and try again.".into();
                return;
            };
            if d["game"].as_str().unwrap_or("") != self.installed {
                self.on_manifest(Some(d));
            } else {
                self.manifest = d;
                if !self.is_running() { self.launch(); }
            }
            return;
        }
        if valid {
            let d = data.unwrap();
            match Self::for_platform(&d) {
                Some(m) => self.on_manifest(Some(m)),
                None => {
                    self.manifest = Value::Null;
                    self.check_failed = true;
                    self.status = if self.cur == "mining" {
                        "There's no Linux build of this version yet. Try again later, or play in the browser: floppamining.com/play/".into()
                    } else {
                        format!("There's no Linux build of {} yet.", game(&self.cur).title)
                    };
                }
            }
        } else if self.manifest.is_null() {
            self.check_failed = true;
        }
    }

    fn on_manifest(&mut self, d: Option<Value>) {
        let Some(d) = d else { return };
        self.manifest = d.clone();
        self.mstore.insert(self.cur.clone(), d.clone());
        if self.cur == "mining" {
            self.maybe_notify(&d);
            if let Some(e) = d.get("exe").and_then(|v| v.as_str()) {
                self.cfg.set_str("launcher", "exe", e);
                self.cfg.save();
            }
        }
        self.installed = installed_version(&self.cur, self.exe_name());
        self.check_failed = false;
        let g = d["game"].as_str().unwrap_or("").to_string();
        if !self.busy && !self.game_running && self.installed != g {
            self.start_download(); // install / update the game right away
        }
    }

    fn switch_game(&mut self, id: &str) {
        if self.busy || id == self.cur || !GAMES.iter().any(|g| g.id == id) { return; }
        self.cur = id.to_string();
        self.cfg.set_str("launcher", "game", id);
        self.cfg.save();
        self.manifest = self.mstore.get(id).and_then(Self::for_platform).unwrap_or(Value::Null);
        self.installed = installed_version(&self.cur, self.exe_name());
        self.game_running = self.is_running();
        self.check_failed = false;
        self.status = "Checking for updates...".into();
        self.fetch(id, false);
        self.fetch_news();
    }

    fn check_now(&mut self) {
        if self.busy { return; }
        self.status = "Checking for updates...".into();
        self.fetch(&self.cur.clone(), false);
    }

    // ------------------------------------------------------------ install / play
    // ---------------------------------------------------------------- FloppaChat (3.2)
    fn fetch_chat(&self) {
        let (tx, ctx, url) = (self.tx.clone(), self.ctx.clone(), format!("{}{}", site(), CHAT_MANIFEST));
        std::thread::spawn(move || {
            let _ = tx.send(Msg::ChatManifest(get_json(&url)));
            ctx.request_repaint();
        });
    }

    fn on_chat_manifest(&mut self, d: Option<Value>) {
        self.chat = d.as_ref().and_then(Self::for_platform).filter(|m| m.get("version").is_some() && m.get("url").is_some()).unwrap_or(Value::Null);
        self.chat_installed = chat_installed();
        // the updater: a new FloppaChat installs by itself while it is closed (an open one is updated next time)
        if !self.chat_installed.is_empty() && self.chat_update() && !self.chat_busy && !chat_running() {
            self.chat_install(false);
        }
        if std::mem::take(&mut self.chat_click) { self.on_chat(); }
    }

    fn chat_update(&self) -> bool {
        let v = self.chat["version"].as_str().unwrap_or("");
        !v.is_empty() && v != self.chat_installed
    }

    fn chat_install(&mut self, open: bool) {
        let m = self.chat.clone();
        let (url, sha, ver) = (m["url"].as_str().unwrap_or("").to_string(), m["sha256"].as_str().unwrap_or("").to_string(), m["version"].as_str().unwrap_or("").to_string());
        if url.is_empty() { return; }
        self.chat_busy = true;
        self.chat_err.clear();
        self.chat_prog = Some((0, m["size"].as_u64().unwrap_or(0)));
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let last = std::cell::Cell::new(Instant::now() - Duration::from_secs(1));
            let p = |got: u64, total: u64| {
                if last.get().elapsed() >= Duration::from_millis(250) {
                    last.set(Instant::now());
                    let _ = tx.send(Msg::ChatProgress(got, total));
                    ctx.request_repaint();
                }
            };
            let result = install_into(&chat_dir(), "floppachat_download.zip", &url, &sha, &ver, chat_exe(), &p);
            let _ = tx.send(Msg::ChatInstalled { result, open });
            ctx.request_repaint();
        });
    }

    /// The FloppaChat button: open it (or bring it to the front), install it, or update it first.
    fn on_chat(&mut self) {
        if self.chat_busy { return; }
        if chat_running() { chat_show(); return; }
        if self.chat_installed.is_empty() || self.chat_update() {
            if self.chat.is_null() {
                if self.chat_installed.is_empty() { self.chat_err = "FloppaChat couldn't be loaded. Check your internet and click again.".into(); self.fetch_chat(); return; }
            } else {
                self.chat_install(true);
                return;
            }
        }
        self.chat_err.clear();
        if let Err(e) = chat_start() { self.chat_err = e; }
    }

    fn chat_label(&self) -> String {
        if self.chat_busy {
            return match self.chat_prog { Some((g, t)) if t > 0 => format!("💬 FloppaChat {}%", (g * 100 / t).min(100)), _ => "💬 FloppaChat ...".into() };
        }
        if !self.chat_err.is_empty() { return "💬 FloppaChat · try again".into(); }
        if self.chat_installed.is_empty() { "💬 Get FloppaChat".into() } else if self.chat_update() { "💬 FloppaChat · update".into() } else { "💬 FloppaChat".into() }
    }

    fn start_download(&mut self) {
        let m = self.manifest.clone();
        let (url, sha, ver) = (m["url"].as_str().unwrap_or("").to_string(), m["sha256"].as_str().unwrap_or("").to_string(), m["game"].as_str().unwrap_or("").to_string());
        if url.is_empty() { return; }
        self.busy = true;
        self.progress = Some((0, m["size"].as_u64().unwrap_or(0)));
        self.speed = 0.0;
        self.last_prog = (Instant::now(), 0);
        self.status = format!("Downloading {}...", vname_for(&self.cur, &ver));
        let (tx, ctx, id, exe) = (self.tx.clone(), self.ctx.clone(), self.cur.clone(), self.exe_name().to_string());
        std::thread::spawn(move || {
            let last = std::cell::Cell::new(Instant::now() - Duration::from_secs(1));
            let p = |got: u64, total: u64| {
                if last.get().elapsed() >= Duration::from_millis(250) {
                    last.set(Instant::now());
                    let _ = tx.send(Msg::Progress(got, total));
                    ctx.request_repaint();
                }
            };
            let result = install(&id, &url, &sha, &ver, &exe, &p);
            let _ = tx.send(Msg::Installed { id, result });
            ctx.request_repaint();
        });
    }

    /// 3.1.2 (Kian): one Floppa game at a time, so the profile can say which game you're in.
    /// Another game that is still running (its own lock port, or the process the launcher started).
    fn other_running(&mut self) -> Option<&'static str> {
        let cur = self.cur.clone();
        let child_game = match &mut self.child {
            Some((id, c)) => if matches!(c.try_wait(), Ok(None)) { id.clone() } else { String::new() },
            None => String::new(),
        };
        for g in GAMES.iter() {
            if g.id == cur { continue; }
            let on = match g.id {
                "mining" => mining_running(),
                "mystery" => port_held(MYSTERY_LOCK_PORT) || child_game == "mystery",
                _ => child_game == g.id,
            };
            if on { return Some(g.title); }
        }
        None
    }

    fn on_play(&mut self) {
        if self.busy || self.is_running() { return; }
        if let Some(t) = self.other_running() {
            self.status = format!("{t} is still running. Close it first: one Floppa game at a time.");
            return;
        }
        if self.manifest.is_null() {
            if self.check_failed {
                self.check_failed = false;
                self.fetch(&self.cur.clone(), false);
            }
            return;
        }
        // check once more right before starting: a new game or launcher may be out since the last check
        self.busy = true;
        self.status = "Checking for updates...".into();
        self.fetch(&self.cur.clone(), true);
    }

    fn launch(&mut self) {
        if let Some(t) = self.other_running() {
            self.status = format!("{t} is still running. Close it first: one Floppa game at a time.");
            return;
        }
        let exe = game_dir(&self.cur).join(self.exe_name());
        match std::process::Command::new(&exe).arg(format!("--launcher={LAUNCHER_VERSION}")).current_dir(game_dir(&self.cur)).spawn() {
            Ok(child) => {
                if self.cur != "mining" { self.child = Some((self.cur.clone(), child)); }
                self.status = format!("Starting {}...", game(&self.cur).title);
                self.running_check_at = Instant::now() + Duration::from_secs(4);
                if self.cfg.get_bool("launcher", "quit_on_play", false) {
                    self.quit_at = Some(Instant::now() + Duration::from_millis(1500));
                } else {
                    // stay in the tray while the game runs (shows IN GAME, keeps checking for updates)
                    let ctx = self.ctx.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_secs(2));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                        ctx.request_repaint();
                    });
                    self.hidden = true;
                }
            }
            Err(_) => self.status = "The game couldn't start. Try Settings → Repair.".into(),
        }
    }

    fn is_running(&mut self) -> bool {
        if self.cur != "mining" {
            // Felony and Mystery have no lock port: the launcher watches the process it started
            let cur = self.cur.clone();
            if cur == "mystery" && port_held(MYSTERY_LOCK_PORT) { return true; }   // also when started without the launcher
            return match &mut self.child {
                Some((id, c)) if *id == cur => matches!(c.try_wait(), Ok(None)),
                _ => false,
            };
        }
        mining_running()
    }

    fn self_update(&mut self, d: &Value) {
        let url = d.get(if IS_LINUX { "linux" } else { "launcher_url" }).and_then(|v| if IS_LINUX { v.get("launcher_url") } else { Some(v) })
            .and_then(|v| v.as_str()).unwrap_or("").to_string();
        let sha = (if IS_LINUX { d.get("linux").and_then(|l| l.get("launcher_sha256")) } else { d.get("launcher_sha256") })
            .and_then(|v| v.as_str()).unwrap_or("").to_string();
        if url.is_empty() { return; }
        self.self_updating = true;
        self.busy = true;
        self.status = format!("Updating the launcher to {}...", d["launcher"].as_str().unwrap_or(""));
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            if self_update(&url, &sha).is_err() {
                let _ = tx.send(Msg::SelfUpdateFailed);
                ctx.request_repaint();
            }
        });
    }

    fn maybe_notify(&mut self, d: &Value) {
        if !self.cfg.get_bool("win", "notify", false) { return; }
        let g = d["game"].as_str().unwrap_or("").to_string();
        let last = self.cfg.get_str("win", "notified", "");
        if last.is_empty() {
            self.cfg.set_str("win", "notified", &g); // first check after turning it on: remember, don't notify old news
            self.cfg.save();
            return;
        }
        if !g.is_empty() && g != last && newer(&g, &last) {
            self.cfg.set_str("win", "notified", &g);
            self.cfg.save();
            let how = if self.game_running { "It installs when you close the game." } else { "Open the launcher to play it." };
            toast(&format!("Floppa Mining {} is out", vname(&g)), how);
        }
    }

    // ------------------------------------------------------------ account (the game's own login file)
    fn acc_cfg(&self) -> Cfg { Cfg::load(mining_user_dir().join("account.cfg")) }

    fn load_account(&mut self) {
        let c = self.acc_cfg();
        let (id, tok) = (c.get_str("account", "id", ""), c.get_str("account", "token", ""));
        if id == self.acc_id && tok == self.acc_token && (!self.acc_name.is_empty() || id.is_empty()) { return; }
        self.acc_id = id;
        self.acc_token = tok;
        self.acc_name.clear();
        if self.acc_id.is_empty() || self.acc_token.is_empty() { return; }
        self.call("me", "GET", "/v1/me", None);
    }

    fn save_game_login(&mut self, id: &str, token: &str) {
        let mut c = self.acc_cfg();
        c.set_str("account", "id", id);
        c.set_str("account", "token", token);
        c.save();
        self.acc_id = id.into();
        self.acc_token = token.into();
        if id.is_empty() { self.acc_name.clear(); }
    }

    /// 3.3: friends see IN LAUNCHER (FloppaNet) while the window shows and you are logged in: a beat every 60 s (the
    /// account server counts one 150 s); hidden in the tray or with a game running = a "leaving" beat, then none.
    fn here_beat(&mut self) {
        let show = !self.hidden && !self.acc_id.is_empty() && !self.acc_token.is_empty();
        if show && self.here_at.map_or(true, |t| t.elapsed() >= Duration::from_secs(60)) {
            self.here_at = Some(Instant::now());
            self.call("here", "POST", "/v1/here", Some(serde_json::json!({"app": "launcher"})));
        } else if !show && self.here_at.is_some() {
            self.here_at = None;
            if !self.acc_id.is_empty() { self.call("here", "POST", "/v1/here", Some(serde_json::json!({"app": "launcher", "leaving": true}))); }
        }
    }

    fn call(&self, kind: &'static str, method: &'static str, path: &'static str, body: Option<Value>) {
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        let (id, tok) = (self.acc_id.clone(), self.acc_token.clone());
        let base = api_url().to_string();
        std::thread::spawn(move || {
            let (code, data) = api(method, &format!("{base}{path}"), body, Some((&id, &tok)));
            let _ = tx.send(Msg::Api { kind, code, data });
            ctx.request_repaint();
        });
    }

    fn err_text(code: u16, d: &Value) -> String {
        if code == 0 { return "Can't reach the server. Check your internet.".into(); }
        if code == 429 { return "Too many tries. Wait a few minutes.".into(); }
        let e = d.get("error").and_then(|v| v.as_str()).unwrap_or("");
        if e == "banned" { return format!("This account is banned: {}", d.get("reason").and_then(|v| v.as_str()).unwrap_or("")); }
        if e.is_empty() { format!("Server problem ({code})") } else { e.to_string() }
    }

    fn on_api(&mut self, kind: &str, code: u16, d: Value) {
        match kind {
            "me" => {
                if code == 200 { self.acc_name = d["name"].as_str().unwrap_or("").into(); }
                else if code == 401 { self.save_game_login("", ""); } // that login was logged out somewhere else
            }
            "login" | "signup" => {
                self.f_busy = false;
                if code != 200 { self.f_msg = Self::err_text(code, &d); return; }
                self.save_game_login(d["id"].as_str().unwrap_or(""), d["token"].as_str().unwrap_or(""));
                self.acc_name = d["name"].as_str().unwrap_or(&self.f_name).to_string();
                self.f_pw.clear();
                self.f_pw2.clear();
                self.page = if kind == "signup" { Page::Code(d["recovery"].as_str().unwrap_or("").into()) } else { Page::None };
            }
            "logout" => {
                self.save_game_login("", "");
                self.page = Page::None;
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------ messages
    fn pump(&mut self) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Manifest { id, data, play } => self.on_manifest_for(id, data, play),
                Msg::ChatManifest(d) => self.on_chat_manifest(d),
                Msg::ChatProgress(got, total) => self.chat_prog = Some((got, if total > 0 { total } else { self.chat["size"].as_u64().unwrap_or(0) })),
                Msg::ChatInstalled { result, open } => {
                    self.chat_busy = false;
                    self.chat_prog = None;
                    self.chat_installed = chat_installed();
                    match result {
                        Ok(()) if open => if let Err(e) = chat_start() { self.chat_err = e },
                        Ok(()) => {}
                        Err(e) => self.chat_err = e,
                    }
                }
                Msg::News { gen, items } if gen == self.news_gen => {
                    self.news_loaded = true;
                    self.news = items.iter().map(|it| Card {
                        title: it["title"].as_str().unwrap_or("").into(), date: it["date"].as_str().unwrap_or("").into(),
                        v: it["v"].as_str().unwrap_or("").into(), url: it["url"].as_str().unwrap_or("/updates/").into(),
                        tex: it["image"].as_str().filter(|u| !u.is_empty()).and_then(|u| self.img_cache.get(&abs(u)).cloned()),
                    }).collect();
                }
                Msg::NewsImage { gen, idx, url, img } => {
                    let t = self.img_cache.entry(url.clone()).or_insert_with(|| self.ctx.load_texture(format!("news:{url}"), img, egui::TextureOptions::LINEAR)).clone();
                    if gen == self.news_gen { if let Some(c) = self.news.get_mut(idx) { c.tex = Some(t); } }
                }
                Msg::Progress(got, total) => {
                    let dt = self.last_prog.0.elapsed().as_secs_f64();
                    if dt > 0.2 {
                        let s = (got.saturating_sub(self.last_prog.1)) as f64 / dt;
                        self.speed = if self.speed > 0.0 { self.speed * 0.5 + s * 0.5 } else { s };
                        self.last_prog = (Instant::now(), got);
                    }
                    let total = if total > 0 { total } else { self.manifest["size"].as_u64().unwrap_or(0) };
                    self.progress = Some((got, total));
                    let left = if self.speed > 1000.0 && total > got { let s = ((total - got) as f64 / self.speed) as u64; format!(" · {}:{:02} left", s / 60, s % 60) } else { String::new() };
                    self.status = format!("Downloading {}: {} / {} · {:.1} MB/s{}", vname_for(&self.cur, self.manifest["game"].as_str().unwrap_or("")), mb(got), mb(total), self.speed / 1048576.0, left);
                }
                Msg::Installed { id, result } => {
                    self.busy = false;
                    self.progress = None;
                    if id == self.cur {
                        self.installed = installed_version(&self.cur, self.exe_name());
                        self.status = match result { Ok(()) => format!("{} is installed. Have fun!", vname_for(&self.cur, &self.installed)), Err(e) => e };
                    }
                }
                Msg::SelfUpdateFailed => {
                    self.self_updating = false;
                    self.busy = false; // the "new launcher" banner shows as a fallback
                }
                Msg::Api { kind, code, data } => self.on_api(kind, code, data),
                Msg::Show => self.show_window(),
                Msg::Update(g) => {
                    self.show_window();
                    self.switch_game(&g); // Floppa Mining sends "update", Floppa Mystery "update mystery"
                    self.status = "A new version is out: it installs when the game has closed...".into();
                    self.running_check_at = Instant::now() + Duration::from_millis(500);
                    self.check_now();
                }
                Msg::Tray(n) => match n {
                    0 => self.show_window(),
                    1 => { self.show_window(); self.on_play(); }
                    2 => { self.show_window(); self.check_now(); }
                    _ => std::process::exit(0),
                },
                Msg::Background { id, img } => {
                    let k = game(&id).id;
                    self.bg.insert(k, self.ctx.load_texture(k, img, egui::TextureOptions::LINEAR));
                }
                Msg::ChatAsk => self.chat_wait = Some(Instant::now() + Duration::from_secs(15)),
                Msg::Tick => {
                    // 3.4: FloppaChat asked for its update and is closing: once it is gone, update and open it
                    if let Some(until) = self.chat_wait {
                        if !chat_running() { self.chat_wait = None; self.chat_click = true; self.fetch_chat(); }
                        else if Instant::now() > until { self.chat_wait = None; chat_show(); }
                    }
                }
                _ => {}
            }
        }
        if Instant::now() >= self.recheck_at {
            self.recheck_at = Instant::now() + Duration::from_secs(600); // look for new versions every 10 minutes
            if !self.busy { self.fetch_all(); }
        }
        if Instant::now() >= self.running_check_at && !self.busy {
            self.running_check_at = Instant::now() + Duration::from_millis(1500);
            let run = self.is_running();
            if run != self.game_running {
                self.game_running = run;
                if !run {
                    self.load_account(); // the game may have logged in or out
                    let g = self.manifest["game"].as_str().unwrap_or("").to_string();
                    if !self.manifest.is_null() && self.installed != g { self.start_download(); } // an update came out while the game was open
                }
            }
        }
        if let Some(t) = self.quit_at { if Instant::now() >= t { std::process::exit(0); } }
    }

    // ------------------------------------------------------------ text for the buttons
    fn play_state(&self) -> (String, bool, String) {
        if self.busy {
            return (if self.progress.is_some() { "...".into() } else { "CHECKING".into() }, false, self.status.clone());
        }
        if self.game_running {
            return ("IN GAME".into(), false, format!("{} is running. Updates install after you close it.", game(&self.cur).title));
        }
        if self.manifest.is_null() {
            let s = if self.check_failed && self.status.starts_with("There's no Linux") { self.status.clone() }
                else if self.check_failed { "Can't reach floppamining.com. Check your internet, then click RETRY.".into() }
                else { "Checking for updates...".into() };
            return (if self.check_failed { "RETRY".into() } else { "CHECKING".into() }, self.check_failed, s);
        }
        let online = self.manifest["game"].as_str().unwrap_or("");
        if self.installed.is_empty() || online != self.installed {
            let s = if self.status.starts_with("The ") || self.status.starts_with("Couldn") { self.status.clone() }
                else { format!("{} ({})", vname_for(&self.cur, online), mb(self.manifest["size"].as_u64().unwrap_or(0))) };
            return (if self.installed.is_empty() { "INSTALL".into() } else { "UPDATE".into() }, true, s);
        }
        let s = if self.status.contains("installed") || self.status.starts_with("Starting") || self.status.starts_with("Everything") || self.status.starts_with("The game couldn't") { self.status.clone() } else { "Up to date. Ready to play".into() };
        ("PLAY".into(), true, s)
    }
}

// ---------------------------------------------------------------- drawing helpers
fn font(size: f32) -> FontId { FontId::new(size, FontFamily::Proportional) }

fn outlined(p: &egui::Painter, pos: Pos2, text: &str, size: f32, col: Color32, outline: f32) {
    let f = font(size);
    for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (-0.7, -0.7), (0.7, 0.7), (-0.7, 0.7), (0.7, -0.7)] {
        p.text(pos + Vec2::new(dx * outline, dy * outline), Align2::LEFT_TOP, text, f.clone(), Color32::from_black_alpha(204));
    }
    p.text(pos, Align2::LEFT_TOP, text, f, col);
}

#[derive(Clone, Copy)]
enum Look { Dark, Gold }

fn button(ui: &mut egui::Ui, rect: Rect, text: &str, size: f32, look: Look, enabled: bool) -> egui::Response {
    let r = ui.allocate_rect(rect, if enabled { Sense::click() } else { Sense::hover() });
    let hover = enabled && r.hovered();
    let p = ui.painter();
    let (fill, stroke, col) = match (look, enabled) {
        (Look::Gold, false) => (Color32::from_rgb(89, 84, 77), Stroke::NONE, Color32::from_white_alpha(128)),
        (Look::Gold, true) => (if r.is_pointer_button_down_on() { Color32::from_rgb(217, 178, 87) } else if hover { Color32::from_rgb(255, 218, 133) } else { GOLD }, Stroke::NONE, INK),
        (Look::Dark, _) => (
            if hover { Color32::from_rgba_unmultiplied(38, 33, 20, 204) } else { Color32::from_black_alpha(140) },
            Stroke::new(1.0_f32, if hover { GOLD } else { Color32::from_white_alpha(31) }),
            if !enabled { Color32::from_white_alpha(90) } else if hover { GOLD } else { Color32::WHITE },
        ),
    };
    let rad = if matches!(look, Look::Gold) && rect.height() > 60.0 { 14 } else { 10 };
    p.rect(rect, CornerRadius::same(rad), fill, stroke, StrokeKind::Inside);
    p.text(rect.center(), Align2::CENTER_CENTER, text, font(size), col);
    if hover { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
    r
}

fn wrapped(p: &egui::Painter, pos: Pos2, text: &str, size: f32, col: Color32, width: f32) -> f32 {
    let g = p.layout(text.to_string(), font(size), col, width);
    let h = g.size().y;
    p.galley(pos, g, col);
    h
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.pump();
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.cfg.get_bool("win", "tray_on_close", false) && !self.busy {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.to_tray(); // still checks for updates in the background
            } else {
                std::process::exit(0); // close for real: a download thread or the tray icon never keeps it alive
            }
        }
        self.here_beat();
        if self.hidden && self.shot.is_none() {
            return; // in the tray: only the heartbeat wakes us (update checks, game running); nothing is drawn
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| self.draw(ui));
        ctx.request_repaint_after(Duration::from_millis(if self.busy { 100 } else { 1000 }));
        if let Some(path) = self.shot.clone() {
            let at = *self.shot_at.get_or_insert(Instant::now() + Duration::from_secs(4));
            if Instant::now() >= at && !self.busy {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
                self.shot = None;
                self.shot_at = Some(Instant::now());
                let _ = path;
            }
            ctx.request_repaint_after(Duration::from_millis(200));
        }
        let shots: Vec<_> = ctx.input(|i| i.events.iter().filter_map(|e| if let egui::Event::Screenshot { image, .. } = e { Some(image.clone()) } else { None }).collect());
        for img in shots {
            if let Some(path) = arg_val("--shot=") {
                let _ = image::RgbaImage::from_raw(img.size[0] as u32, img.size[1] as u32, img.as_raw().to_vec()).map(|i| i.save(&path));
                println!("shot saved: {path} | status: {} | button: {}", self.play_state().2, self.play_state().0);
                std::process::exit(0);
            }
        }
    }
}

impl App {
    fn draw(&mut self, ui: &mut egui::Ui) {
        let full = Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 640.0));
        let p = ui.painter().clone();
        p.rect_filled(full, CornerRadius::ZERO, Color32::from_rgb(0x0b, 0x0b, 0x0d));
        if let Some(t) = self.bg.get(self.cur.as_str()).or_else(|| self.bg.get("mining")) {
            // fill the window without stretching: crop the picture's sides (or top and bottom) to the window's shape
            let [w, h] = t.size();
            let (img_a, win_a) = (w as f32 / h.max(1) as f32, full.width() / full.height());
            let uv = if img_a > win_a {
                let c = (1.0 - win_a / img_a) * 0.5;
                Rect::from_min_max(Pos2::new(c, 0.0), Pos2::new(1.0 - c, 1.0))
            } else {
                let c = (1.0 - img_a / win_a) * 0.5;
                Rect::from_min_max(Pos2::new(0.0, c), Pos2::new(1.0, 1.0 - c))
            };
            p.image(t.id(), full, uv, Color32::from_rgb(140, 140, 153));
        }
        // shade: light at the top, dark at the bottom
        let mut mesh = egui::Mesh::default();
        let (top, bot) = (Color32::from_black_alpha(38), Color32::from_black_alpha(235));
        mesh.colored_vertex(full.left_top(), top);
        mesh.colored_vertex(full.right_top(), top);
        mesh.colored_vertex(full.right_bottom(), bot);
        mesh.colored_vertex(full.left_bottom(), bot);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        p.add(egui::Shape::mesh(mesh));

        // top bar
        let g = game(&self.cur);
        outlined(&p, Pos2::new(32.0, 20.0), g.title, 38.0, GOLD, 2.5);
        let tw = p.layout_no_wrap(g.title.into(), font(38.0), GOLD).size().x;
        let tag = format!("{}  ·  {} EDITION", g.tag, edition().to_uppercase());
        let tag_w = p.layout_no_wrap(tag.clone(), font(14.0), Color32::WHITE).size().x;
        let mut x = 1068.0;
        let gear = Rect::from_min_size(Pos2::new(x - 46.0, 27.0), Vec2::new(46.0, 40.0));
        if button(ui, gear, "⚙", 20.0, Look::Dark, true).clicked() { self.settings_open = !self.settings_open; }
        x -= 46.0 + 18.0;
        let acc_text = if !self.acc_name.is_empty() { format!("👤 {}", self.acc_name) } else if !self.acc_id.is_empty() { "👤 ...".into() } else { "Log in".into() };
        // 3.2: FloppaChat sits here, next to the settings, and never in the game switcher (it's an app, not a game)
        let mut links: Vec<(String, u8)> = vec![(self.chat_label(), 3), ("Website".into(), 0), (acc_text, 1), ("Patch notes".into(), 2)];
        for (text, kind) in links.drain(..) {
            let w = p.layout_no_wrap(text.clone(), font(17.0), Color32::WHITE).size().x + 28.0;
            let r = Rect::from_min_size(Pos2::new(x - w, 29.0), Vec2::new(w, 36.0));
            let resp = button(ui, r, &text, 17.0, Look::Dark, true);
            let resp = if kind == 3 {
                let tip = if !self.chat_err.is_empty() { self.chat_err.clone() } else if self.chat_installed.is_empty() {
                    format!("The chat of the Floppa games: servers, direct messages and voice calls with your Floppa account.\nClick to install it ({}).", if self.chat["size"].as_u64().unwrap_or(0) > 0 { mb(self.chat["size"].as_u64().unwrap_or(0)) } else { "a few MB".into() })
                } else if self.chat_update() {
                    format!("FloppaChat {} is out (you have {}). It updates by itself while FloppaChat is closed, or now when you click.", self.chat["version"].as_str().unwrap_or(""), self.chat_installed)
                } else { format!("Open FloppaChat {}", self.chat_installed) };
                resp.on_hover_text(tip)
            } else { resp };
            if resp.clicked() {
                match kind {
                    3 => self.on_chat(),
                    0 => open_url(&format!("{}/", g.site)),
                    1 => { self.page = if self.acc_id.is_empty() { Page::Login } else { Page::Account }; self.f_msg.clear(); }
                    _ => open_url(g.notes),
                }
            }
            x -= w + 18.0;
        }
        // 3.3.1: the tag line next to the title only while it fits before the buttons (the FloppaChat button made the
        // row longer: they overlapped); otherwise under the title
        let tag_pos = if 32.0 + tw + 18.0 + tag_w <= x { Pos2::new(32.0 + tw + 18.0, 38.0) } else { Pos2::new(34.0, 66.0) };
        p.text(tag_pos, Align2::LEFT_TOP, tag, font(14.0), Color32::from_white_alpha(178));

        // new-launcher banner (only if the self-update didn't work)
        let lv = self.mstore.get("mining").and_then(|m| m.get("launcher")).and_then(|v| v.as_str()).unwrap_or("0").to_string();
        if newer(&lv, LAUNCHER_VERSION) && !self.self_updating {
            let r = Rect::from_min_size(Pos2::new(32.0, 84.0), Vec2::new(1036.0, 36.0));
            if button(ui, r, "A new launcher is out: click to download it", 15.0, Look::Dark, true).clicked() { open_url(&format!("{}/download/", site())); }
        }

        // game switcher
        // 3.1.6: only the games that show (Settings: "Hide in-work games"); one game alone needs no switcher
        let shown = shown_games(self.cfg.get_bool("launcher", "hide_in_work", true));
        for (i, gg) in shown.iter().enumerate().filter(|_| shown.len() > 1) {
            let r = Rect::from_min_size(Pos2::new(32.0 + i as f32 * 200.0, 130.0), Vec2::new(190.0, 44.0));
            let on = gg.id == self.cur;
            if button(ui, r, gg.title, 18.0, if on { Look::Gold } else { Look::Dark }, !self.busy || on).clicked() && !on {
                self.switch_game(gg.id);
            }
        }

        // news
        p.text(Pos2::new(32.0, 300.0), Align2::LEFT_TOP, "LATEST UPDATES", font(15.0), GOLD);
        if self.news.is_empty() {
            p.text(Pos2::new(32.0, 330.0), Align2::LEFT_TOP, if self.news_loaded { "Couldn't load the patch notes." } else { "Loading patch notes..." }, font(16.0), Color32::from_white_alpha(153));
        }
        let mut open = None;
        for (i, c) in self.news.iter().enumerate() {
            let r = Rect::from_min_size(Pos2::new(32.0 + i as f32 * 350.0, 326.0), Vec2::new(334.0, 176.0));
            let resp = ui.allocate_rect(r, Sense::click());
            let hover = resp.hovered();
            p.rect(r, CornerRadius::same(14), if hover { Color32::from_rgba_unmultiplied(31, 26, 15, 235) } else { PANEL }, Stroke::new(1.0_f32, if hover { GOLD } else { Color32::from_white_alpha(31) }), StrokeKind::Inside);
            if let Some(t) = &c.tex {
                let ir = Rect::from_min_size(r.min + Vec2::new(1.0, 1.0), Vec2::new(332.0, 104.0));
                let sz = t.size_vec2();
                let (ra, ia) = (ir.width() / ir.height(), sz.x / sz.y);
                let uv = if ia > ra { let w = ra / ia; Rect::from_min_max(Pos2::new((1.0 - w) / 2.0, 0.0), Pos2::new((1.0 + w) / 2.0, 1.0)) }
                    else { let h = ia / ra; Rect::from_min_max(Pos2::new(0.0, (1.0 - h) / 2.0), Pos2::new(1.0, (1.0 + h) / 2.0)) };
                let mut img = egui::epaint::RectShape::filled(ir, CornerRadius { nw: 13, ne: 13, sw: 0, se: 0 }, Color32::WHITE);
                img.brush = Some(std::sync::Arc::new(egui::epaint::Brush { fill_texture_id: t.id(), uv }));
                p.add(img);
            } else {   // 3.0.3: an update without a picture says so instead of an empty box
                let ir = Rect::from_min_size(r.min + Vec2::new(1.0, 1.0), Vec2::new(332.0, 104.0));
                p.rect_filled(ir, CornerRadius { nw: 13, ne: 13, sw: 0, se: 0 }, Color32::from_rgb(20, 18, 15));
                p.text(ir.center(), Align2::CENTER_CENTER, "No preview image for this update", font(13.0), Color32::from_white_alpha(90));
            }
            p.text(r.min + Vec2::new(14.0, 112.0), Align2::LEFT_TOP, format!("{}  ·  {}", c.v, c.date), font(13.0), GOLD);
            wrapped(&p, r.min + Vec2::new(14.0, 130.0), &c.title, 16.0, Color32::WHITE, 306.0);
            if hover { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
            if resp.clicked() { open = Some(abs(&c.url)); }
        }
        if let Some(u) = open { open_url(&u); }

        // bottom bar
        let bottom = Rect::from_min_size(Pos2::new(0.0, 530.0), Vec2::new(1100.0, 110.0));
        p.rect_filled(bottom, CornerRadius::ZERO, PANEL);
        let (btn_text, btn_on, status) = self.play_state();
        p.text(Pos2::new(46.0, 556.0), Align2::LEFT_TOP, &status, font(18.0), Color32::WHITE);
        if let Some((got, total)) = self.progress {
            let br = Rect::from_min_size(Pos2::new(46.0, 584.0), Vec2::new(706.0, 12.0));
            p.rect_filled(br, CornerRadius::same(6), Color32::from_white_alpha(26));
            let f = if total > 0 { (got as f32 / total as f32).clamp(0.0, 1.0) } else { 0.0 };
            if f > 0.0 { p.rect_filled(Rect::from_min_size(br.min, Vec2::new(br.width() * f, 12.0)), CornerRadius::same(6), GOLD); }
        }
        let online = self.manifest["game"].as_str().unwrap_or("");
        // 3.0.4 (Kian): a small release date after each version (from the patch notes), always shown, no hover needed
        let dated = |v: &str| -> String {
            let n = vname_for(&self.cur, v);
            match self.news.iter().find(|c| c.v == n && !c.date.is_empty()) {
                Some(c) => format!("{n} ({})", short_date(&c.date)),
                None => n,
            }
        };
        let ver = format!("Installed: {}   ·   Newest: {}   ·   Launcher {} ({})",
            if self.installed.is_empty() { "nothing yet".into() } else { dated(&self.installed) },
            if online.is_empty() { "checking...".into() } else { dated(online) }, LAUNCHER_VERSION, edition());
        p.text(Pos2::new(46.0, 604.0), Align2::LEFT_TOP, ver, font(13.0), Color32::from_white_alpha(128));
        let pr = Rect::from_min_size(Pos2::new(804.0, 552.0), Vec2::new(250.0, 66.0));
        if button(ui, pr, &btn_text, 30.0, Look::Gold, btn_on).clicked() {
            if self.manifest.is_null() || self.installed.is_empty() || online != self.installed {
                if self.manifest.is_null() { self.on_play(); } else { self.start_download(); }
            } else {
                self.on_play();
            }
        }

        if self.settings_open { self.draw_settings(ui, gear); }
        if self.page != Page::None { self.draw_account(ui); }
    }

    fn draw_settings(&mut self, ui: &mut egui::Ui, gear: Rect) {
        let lin = IS_LINUX;
        let toggles: [(&str, &str, &str, bool); 7] = [
            ("launcher", "hide_in_work", "Hide in-work games (public tests and betas)", true),
            ("launcher", "quit_on_play", "Quit the launcher when the game starts (off: it waits in the tray)", false),
            ("win", "notify", if lin { "Notifications for new updates" } else { "Windows notifications for new updates" }, false),
            ("win", "desktop", "Desktop shortcut", true),
            ("win", "startmenu", if lin { "App menu shortcut" } else { "Start menu shortcut" }, true),
            ("win", "autostart", if lin { "Start with the PC (in the tray)" } else { "Start with Windows (in the tray)" }, false),
            ("win", "tray_on_close", "Closing the window keeps it in the tray", false),
        ];
        let actions = ["Check for updates", "Open the game folder", "Repair (download the game again)", "Uninstall the game"];
        let w = 520.0;
        let h = 16.0 + toggles.len() as f32 * 32.0 + 12.0 + actions.len() as f32 * 32.0 + 12.0 + 30.0;
        let panel = Rect::from_min_size(Pos2::new(gear.max.x - w, gear.max.y + 6.0), Vec2::new(w, h));
        let p = ui.painter().clone();
        // click outside closes it
        let outside = ui.ctx().input(|i| i.pointer.any_click() && i.pointer.interact_pos().map(|q| !panel.contains(q) && !gear.contains(q)).unwrap_or(false));
        p.rect(panel, CornerRadius::same(12), Color32::from_rgb(16, 16, 21), Stroke::new(1.0_f32, Color32::from_white_alpha(40)), StrokeKind::Inside);
        let mut y = panel.min.y + 8.0;
        for (sec, key, label, def) in toggles {
            let r = Rect::from_min_size(Pos2::new(panel.min.x + 6.0, y), Vec2::new(w - 12.0, 30.0));
            let resp = ui.allocate_rect(r, Sense::click());
            if resp.hovered() { p.rect_filled(r, CornerRadius::same(6), Color32::from_white_alpha(14)); }
            let on = self.cfg.get_bool(sec, key, def);
            let bx = Rect::from_min_size(Pos2::new(r.min.x + 8.0, r.center().y - 8.0), Vec2::splat(16.0));
            p.rect(bx, CornerRadius::same(4), if on { GOLD } else { Color32::TRANSPARENT }, Stroke::new(1.5_f32, if on { GOLD } else { Color32::from_white_alpha(120) }), StrokeKind::Inside);
            if on { p.text(bx.center(), Align2::CENTER_CENTER, "✔", font(12.0), INK); }
            p.text(Pos2::new(r.min.x + 34.0, r.center().y), Align2::LEFT_CENTER, label, font(15.0), Color32::WHITE);
            if resp.clicked() {
                self.cfg.set_bool(sec, key, !on);
                self.cfg.save();
                if sec == "win" && key != "notify" { apply_system(&self.cfg); }
                // turned on while an in-work game is open: back to the first game that still shows
                if key == "hide_in_work" && !on && !self.busy && !shown_games(true).iter().any(|g| g.id == self.cur) { self.switch_game("mining"); }
                if key == "notify" && !on { toast(APP_NAME, "Notifications are on. You'll get one when a new Floppa Mining update is out."); }
            }
            y += 32.0;
        }
        p.hline(panel.min.x + 10.0..=panel.max.x - 10.0, y + 5.0, Stroke::new(1.0_f32, Color32::from_white_alpha(30)));
        y += 12.0;
        for (i, label) in actions.iter().enumerate() {
            let r = Rect::from_min_size(Pos2::new(panel.min.x + 6.0, y), Vec2::new(w - 12.0, 30.0));
            let enabled = !(i >= 2 && self.busy);
            let resp = ui.allocate_rect(r, if enabled { Sense::click() } else { Sense::hover() });
            if resp.hovered() && enabled { p.rect_filled(r, CornerRadius::same(6), Color32::from_white_alpha(14)); }
            p.text(Pos2::new(r.min.x + 12.0, r.center().y), Align2::LEFT_CENTER, *label, font(15.0), if enabled { Color32::WHITE } else { Color32::from_white_alpha(90) });
            if resp.clicked() {
                self.settings_open = false;
                match i {
                    0 => self.check_now(),
                    1 => open_folder(&game_dir(&self.cur)),
                    2 => if !self.manifest.is_null() { self.start_download(); },
                    _ => {
                        let _ = std::fs::remove_dir_all(game_dir(&self.cur));
                        self.installed.clear();
                    }
                }
            }
            y += 32.0;
        }
        p.hline(panel.min.x + 10.0..=panel.max.x - 10.0, y + 5.0, Stroke::new(1.0_f32, Color32::from_white_alpha(30)));
        p.text(Pos2::new(panel.min.x + 18.0, y + 22.0), Align2::LEFT_CENTER, format!("Launcher {LAUNCHER_VERSION} · {} edition", edition()), font(14.0), Color32::from_white_alpha(110));
        if outside { self.settings_open = false; }
    }

    fn draw_account(&mut self, ui: &mut egui::Ui) {
        let full = Rect::from_min_size(Pos2::ZERO, Vec2::new(1100.0, 640.0));
        let p = ui.painter().clone();
        let _block = ui.allocate_rect(full, Sense::click()); // nothing behind the window can be clicked
        p.rect_filled(full, CornerRadius::ZERO, Color32::from_black_alpha(178));
        let w = 460.0;
        let page = self.page.clone();
        // the window fits its content: drawn first, then the frame goes under it (its height was measured last time)
        let h = self.panel_h.max(200.0);
        let panel = Rect::from_center_size(full.center(), Vec2::new(w, h));
        let frame_idx = p.add(egui::Shape::Noop);
        let x0 = panel.min.x + 22.0;
        let inner = w - 44.0;
        let mut y = panel.min.y + 20.0;
        let title = match &page { Page::Login => "Log in", Page::Signup => "New account", Page::Code(_) => "Your recovery code", Page::Account => "Your account", Page::None => "" };
        p.text(Pos2::new(x0, y), Align2::LEFT_TOP, title, font(28.0), GOLD);
        y += 44.0;
        let field = |ui: &mut egui::Ui, y: &mut f32, s: &mut String, hint: &str, secret: bool| -> egui::Response {
            let r = Rect::from_min_size(Pos2::new(x0, *y), Vec2::new(inner, 44.0));
            let te = egui::TextEdit::singleline(s).hint_text(hint).password(secret).font(font(18.0)).char_limit(if secret { 64 } else { 16 })
                .margin(Vec2::new(12.0, 10.0)).background_color(Color32::from_rgb(24, 24, 29));
            let resp = ui.put(r, te);
            *y += 52.0;
            resp
        };
        let note = |y: &mut f32, text: &str, col: Color32, size: f32| {
            if text.is_empty() { return; }
            let hh = wrapped(&p, Pos2::new(x0, *y), text, size, col, inner);
            *y += hh + 10.0;
        };
        let mut go_page: Option<Page> = None;
        match page {
            Page::Login => {
                note(&mut y, "Log in once here and the game starts logged in.", Color32::from_white_alpha(153), 14.0);
                let n = field(ui, &mut y, &mut self.f_name, "Name (letters, numbers or _)", false);
                let pw = field(ui, &mut y, &mut self.f_pw, "Password", true);
                if self.f_name.is_empty() && !n.has_focus() && !pw.has_focus() { n.request_focus(); }
                note(&mut y, &self.f_msg.clone(), Color32::from_rgb(255, 138, 101), 15.0);
                let submit = pw.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let r = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(inner, 46.0));
                if (button(ui, r, "Log in", 18.0, Look::Gold, !self.f_busy).clicked() || submit) && !self.f_busy {
                    let name = self.f_name.trim().to_string();
                    if name.is_empty() || self.f_pw.is_empty() { self.f_msg = "Type your name and password.".into(); }
                    else if !name_ok(&name) { self.f_msg = "Names only have letters (A-Z), numbers and _ (no spaces or other signs).".into(); }
                    else {
                        self.f_busy = true;
                        self.f_msg = "Logging in...".into();
                        self.call("login", "POST", "/v1/login", Some(serde_json::json!({"name": name, "password": self.f_pw})));
                    }
                }
                y += 54.0;
                for (label, pg) in [("Create an account", 1), ("Forgot your password? (website)", 2), ("Close", 3)] {
                    let r = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(inner, 46.0));
                    if button(ui, r, label, 18.0, Look::Dark, true).clicked() {
                        match pg { 1 => go_page = Some(Page::Signup), 2 => open_url(&format!("{}/account/", site())), _ => go_page = Some(Page::None) }
                    }
                    y += 54.0;
                }
            }
            Page::Signup => {
                let n = field(ui, &mut y, &mut self.f_name, "Name", false);
                if self.f_name.is_empty() && !n.has_focus() && !ui.ctx().memory(|m| m.focused().is_some()) { n.request_focus(); }
                field(ui, &mut y, &mut self.f_pw, "Password", true);
                field(ui, &mut y, &mut self.f_pw2, "Password again", true);
                note(&mut y, "Name: 3-16 letters (A-Z), numbers or _ (no spaces or other signs)\nPassword: 8-64 characters, any signs allowed (not your name, nothing easy like 12345678)", Color32::from_white_alpha(153), 14.0);
                note(&mut y, &self.f_msg.clone(), Color32::from_rgb(255, 138, 101), 15.0);
                let r = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(inner, 46.0));
                if button(ui, r, "Create account", 18.0, Look::Gold, !self.f_busy).clicked() {
                    let name = self.f_name.trim().to_string();
                    if !name_ok(&name) { self.f_msg = "Names have 3-16 letters (A-Z), numbers or _ (no spaces or other signs).".into(); }
                    else if self.f_pw.chars().count() < 8 { self.f_msg = "The password needs at least 8 characters.".into(); }
                    else if self.f_pw != self.f_pw2 { self.f_msg = "The passwords are not the same.".into(); }
                    else {
                        self.f_busy = true;
                        self.f_msg = "Creating your account...".into();
                        self.call("signup", "POST", "/v1/signup", Some(serde_json::json!({"name": name, "password": self.f_pw})));
                    }
                }
                y += 54.0;
                note(&mut y, "No email needed. You get a recovery code: write it down, it's the only way to reset your password.", Color32::from_white_alpha(153), 14.0);
                let r = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(inner, 46.0));
                if button(ui, r, "Back", 18.0, Look::Dark, true).clicked() { go_page = Some(Page::Login); }
                y += 46.0;
            }
            Page::Code(code) => {
                note(&mut y, "Write it down or save it somewhere safe. It's the only way to reset your password, and it's shown only now.", Color32::from_white_alpha(204), 15.0);
                let r = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(inner, 56.0));
                p.rect(r, CornerRadius::same(10), Color32::from_rgb(24, 24, 29), Stroke::new(1.0_f32, GOLD), StrokeKind::Inside);
                p.text(r.center(), Align2::CENTER_CENTER, &code, font(24.0), GOLD);
                y += 64.0;
                let r = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(inner, 46.0));
                if button(ui, r, "Copy", 18.0, Look::Dark, true).clicked() { ui.ctx().copy_text(code.clone()); }
                y += 54.0;
                let r = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(inner, 46.0));
                if button(ui, r, "I wrote it down", 18.0, Look::Gold, true).clicked() { go_page = Some(Page::None); }
                y += 46.0;
            }
            Page::Account => {
                let name = if self.acc_name.is_empty() { "...".to_string() } else { self.acc_name.clone() };
                note(&mut y, &format!("Logged in as {name}. The game starts logged in with this account."), Color32::WHITE, 17.0);
                let r = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(inner, 46.0));
                if button(ui, r, "Account page on floppamining.com", 18.0, Look::Dark, true).clicked() { open_url(&format!("{}/account/", site())); }
                y += 54.0;
                let running = self.cur == "mining" && self.game_running;
                let r = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(inner, 46.0));
                if button(ui, r, "Log out", 18.0, Look::Dark, !running && !self.f_busy).clicked() {
                    self.f_busy = true;
                    self.call("logout", "POST", "/v1/logout", None);
                }
                y += 54.0;
                if running { note(&mut y, "Close the game to log out.", Color32::from_white_alpha(153), 14.0); }
                let r = Rect::from_min_size(Pos2::new(x0, y), Vec2::new(inner, 46.0));
                if button(ui, r, "Close", 18.0, Look::Dark, true).clicked() { go_page = Some(Page::None); }
                y += 46.0;
            }
            Page::None => {}
        }
        let real_h = (y - panel.min.y).max(120.0) + 12.0;
        p.set(frame_idx, egui::epaint::RectShape::new(Rect::from_min_size(panel.min, Vec2::new(w, real_h)), CornerRadius::same(18), Color32::from_rgb(14, 14, 18), Stroke::new(1.0_f32, Color32::from_rgb(178, 146, 71)), StrokeKind::Inside));
        if (real_h - self.panel_h).abs() > 1.0 { self.panel_h = real_h; ui.ctx().request_repaint(); }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) { go_page = Some(Page::None); }
        if let Some(pg) = go_page {
            if pg == Page::None { self.f_busy = false; }
            self.f_msg.clear();
            self.page = pg;
        }
    }
}

fn name_ok(n: &str) -> bool {
    (3..=16).contains(&n.chars().count()) && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}
