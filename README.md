# Floppa Launcher

**3.0 is written in Rust (`rust/`; 3.2.0 adds FloppaChat, see "FloppaChat" at the end; 3.3.0 tells friends IN LAUNCHER, see the end):** same screen, games, settings and files as the Godot launcher (main.gd, kept as the
fallback until 3.0 is confirmed on Windows), but a ~5 MB exe instead of ~105 MB, no PowerShell, no game engine.
Build: `cd rust && cargo build --release` (Linux) and `cargo build --release --target x86_64-pc-windows-gnu` (Windows,
needs `mingw-w64`). Zip `FloppaLauncher.exe` / `FloppaLauncher.x86_64` as `builds/FloppaLauncher-<v>.zip` / `-linux.zip`.
Tests: `cargo test --release` (api_test needs the game server on 127.0.0.1:8799: floppatreasure-game/server `npx wrangler dev --port 8799 --local`),
screenshots: `FloppaLauncher --site=http://127.0.0.1:8124 --no-self-update --open=settings|login|signup --shot=/tmp/x.png`.
Updating from Godot 2.x: the old launcher downloads the zip, swaps in the new exe and starts it; the Rust launcher waits
for the old one to let go of port 47732 when a `.old` exe is next to it (it starts in milliseconds, Godot took seconds).

(Called "Floppa Mining Launcher" until 1.7.) Small launcher for **Floppa Mining** (free to play, Windows), since 2.5 **Floppa Felony** (public test) and since 3.1 **Floppa Mystery** (open beta since 3.1.3): switch games under the top bar. It installs the game, updates it and shows the newest patch notes.
Download: https://floppamining.com/download/

- Made with Godot 4.8 (`main.gd` is the whole launcher).
- Reads https://floppamining.com/launcher/latest.json (game version, zip URL, size, sha256) and `/launcher/news.json`.
- One launcher at a time (127.0.0.1:47732). The game sends "update" there when a new version is out (or starts the launcher with --update from the path in launcher_path.txt, which the launcher writes next to the game's login file).
- Stays in the tray while the game runs (button shows IN GAME), keeps checking for updates; optional Windows notifications for new updates (Settings, off by default). Icons are PNG/ICO (SVG text can't be drawn by Godot: blank icons before 1.8).
- Installs to `%APPDATA%\FloppaMining\game`, checks the sha256 of every download.
- The game and launcher builds are in `builds/` (GitHub Releases can't be created from the AI sessions). Delete builds older than the last 2 to keep the repo small.
- **Repo size:** git keeps every deleted zip in the history (GitHub blocks at 5 GB). Cleaned with `git filter-repo --path-glob 'builds/*.zip' --invert-paths` in a fresh full clone (`pip install git-filter-repo`), the current zips re-added in one commit, `git push --force-with-lease=main:<old tip>` (2026-11-16: 4.3 GB to 284 MB; 2026-11-20: 4.0 GB to about 650 MB, and again at 1.08 GB and at about 1.09 GB (Floppa Mystery 0.1.16, to about 670 MB); it grows about 2.5 GB a day on busy release days). After a release check `gh api repos/KianFloppa/floppamining-launcher --jq .size` (KB; `du -sh .git` is wrong in shallow clones); above 1 GB, clean the same way (Kian OK'd it). After a cleanup: fresh clones, or `git pull --rebase` (never a plain merge pull, it brings the old zips back).
  The game's source code is private; only the builds are here.

## Release a new game version (for AI sessions)
1. In the game repo: bump `config/version`, `godot --headless --path . --export-release "Windows" ../fmbuild/game/FloppaMining.exe`
2. Zip it: `FloppaMining-<v>.zip` with `FloppaMining.exe` at the root. Linux (since 1.0.8): `--export-release "Linux" ../fmbuild/game-linux/FloppaMining.x86_64`, zip as `FloppaMining-<v>-linux.zip` (the launcher sets the run flag after unzipping).
3. Put the zip in `builds/` of this repo, push.
4. In floppatreasure-site: update `tools/launcher.json` (game, url, size, sha256, and the `linux` block: url, size, sha256), `python3 tools/build.py`, push.
   Also the browser version (same version!): in floppatreasure-game `python3 tools/web_release.py ../floppatreasure-site`, then `python3 tools/pages.py` + `tools/build.py` in the site and push (it replaces play/g/<old version>/).
   The launcher shows UPDATE as soon as the site is live.

## Test
`godot --path . --script res://tests/shots.gd -- --site=http://127.0.0.1:8124 --out=/tmp/x` with a local copy of the launcher JSON files.

## Linux (launcher 2.0)
Export preset "Linux" -> `FloppaLauncher.x86_64`, zip as `FloppaLauncher-<v>-linux.zip`. latest.json's `linux` block replaces url/size/sha256/exe/launcher_url/launcher_sha256 on Linux. Shortcuts are .desktop files (Desktop, ~/.local/share/applications, ~/.config/autostart with --tray), notifications use notify-send.

## Floppa Felony (launcher 2.5, 2026-11-17)
- Manifest `floppamining.com/launcher/felony.json` {game, url, size, sha256, exe} and news `/launcher/felony_news.json` (full links to felony.floppaland.com), both made by floppatreasure-site `tools/pages.py` from the `felony` block of `tools/launcher.json`. Never edit the two files themselves: every site build writes them again (until 2026-11-20 the block still said 117, so a Mystery release's build turned Felony back to 117).
- Installed in `user://felony` (Mining stays in `user://game`). Felony has no lock port: the launcher watches the process it started.
- Release: in floppaheist-game bump `config/version`, `godot --headless --path . --export-release "Windows Desktop" /tmp/felbuild/FloppaFelony.exe`, zip `FloppaFelony.exe` + `FloppaFelony.pck` as `builds/FloppaFelony-<v>.zip`, then in floppatreasure-site update the `felony` block of `tools/launcher.json` (game, url, size, sha256, and a `news` entry), `python3 tools/build.py`, push. No Linux build yet (the launcher says so on Linux).
- Test: `tests/games_test.gd` (local copy of the site with latest.json + felony.json, see its header).

## Floppa Mystery (launcher 3.1, 2026-11-19; 3.1.1: the Patch notes button per game, `Game.notes`)
- Manifest `floppamining.com/launcher/mystery.json` {game, url, size, sha256, exe, linux {url, size, sha256, exe}} and news
  `/launcher/mystery_news.json`, both made by floppatreasure-site `tools/pages.py` from the `mystery` block of
  `tools/launcher.json`. Versions show as "Test 0.1.0" (`vname_for`).
- Installed in `<launcher folder>/mystery`. Lock port 127.0.0.1:47733 while it runs (3.1.2: PLAY refuses while another Floppa game runs; Mystery and Mining refuse to start while the other runs).
  The game's update screen sends "update mystery" to 127.0.0.1:47732 (plain "update" = Floppa Mining).
- The game reads the Floppa login the launcher writes (Mining's account.cfg), so it starts logged in.
- Release: in floppamystery-game bump `config/version`, export the presets "Windows" (`FloppaMystery.exe`, pck embedded)
  and "Linux" (`FloppaMystery.x86_64`), zip each as `FloppaMystery-<v>.zip` / `-linux.zip`, put them in `builds/`, push,
  then the `mystery` block in the site's `tools/launcher.json` + `python3 tools/build.py` + push.
- Background: see "Backgrounds" below.

## Backgrounds (launcher 3.1.4, 2026-11-22)
Each game's background is the **main picture of its own website** (`Game.home` in core.rs): the first `<img>` with
`fetchpriority="high"` (the hero picture every Floppa site marks for loading speed).
- Mining: floppamining.com, Felony: felony.floppaland.com, Mystery: floppamystery.pages.dev.
- Checked at start and with every update check (10 min). Saved in `<data>\cache\bg\<id>.png` (1280 px) and
  `<id>.txt` (picture address + ETag). A changed picture (new file, or same file changed: ETag) shows up by itself;
  304 = unchanged, nothing downloaded again.
- No internet: the last saved picture, else the built-in `rust/assets/bg.jpg`, `bg_felony.jpg`, `bg_mystery.jpg`
  (copies of the site pictures of 2026-11-22; refresh them when a site gets a new main picture).
- Drawn cropped to the window shape (never stretched), darkened like before.
- **To change a launcher background: change the game site's main picture.** Nothing to do in the launcher.

## Hide in-work games (launcher 3.1.6, 2026-11-22)
Settings has "Hide in-work games (public tests and betas)", **on by default** (Kian: only Floppa Mining should show by
default). `Game.in_work` in core.rs marks the games that are not finished (Felony, Mystery); `shown_games()` is the list
the switcher draws. With one game showing there is no switcher row. A hidden game still shows on a PC where it is
already installed, so updating the launcher takes nobody's game away. When a game is finished: set `in_work: false`.

## FloppaChat (launcher 3.2.0, 2026-11-25)
FloppaChat (the chat of the Floppa games, repo `KianFloppa/floppachat`, desktop app in `desktop/`) is an **app, not a
game**: it never shows in the game switcher. It has its own button in the top bar, next to the settings.
- Button: "💬 Get FloppaChat" (not installed) → downloads, checks the sha256, installs and opens it; "💬 FloppaChat" →
  opens it, or brings an open one to the front; "💬 FloppaChat · update" → a new version is out; "· try again" → the
  last download or start failed (hover for why).
- **Updater:** with every update check (start, every 10 minutes) the launcher reads `floppamining.com/launcher/chat.json`
  and installs a new FloppaChat by itself while FloppaChat is closed. An open FloppaChat is updated after it closes,
  or right away with a click on the button (when it's closed).
- Manifest `/launcher/chat.json` {version, url, size, sha256, exe, linux {url, size, sha256, exe}}, made by
  floppatreasure-site `tools/pages.py` from the `chat` block of `tools/launcher.json` (never edit the file itself).
- Installed in `<launcher folder>/floppachat` (`version.txt` next to it). FloppaChat holds 127.0.0.1:47740 while it runs
  (one at a time; "show" there brings it to the front); the launcher uses that to know whether it's open.
- Zips: `builds/FloppaChat-<v>.zip` (`FloppaChat.exe` + `WebView2Loader.dll`) and `-linux.zip` (`FloppaChat`).
- Release a new FloppaChat app: in floppachat `desktop/` bump the version in Cargo.toml, build both (desktop/README.md),
  zip, put the zips in `builds/`, push; then the `chat` block in floppatreasure-site `tools/launcher.json` +
  `python3 tools/build.py` + push. Only needed when the window program changes: the chat itself is the web app at
  chat.floppaland.com and updates without the launcher.
- Test: `--open=chat` clicks the button as soon as the manifest is in (with `--site=` a local copy of the site files and
  `--shot=`), see floppachat desktop/README.md.

## IN LAUNCHER for friends (launcher 3.3.0, 2026-11-25)
While the window shows and the game's login (account.cfg) is set, the launcher sends POST /v1/here {app: "launcher"}
every 60 s (`here_beat` in main.rs); hidden in the tray or with a game running it sends one `leaving: true` and stops.
Friends then see IN LAUNCHER in FloppaNet 1.11+ (account server 2.1 counts a beat 150 s; a game always wins).
- 3.3.1 (2026-11-25): the tag line ("FREE TO PLAY · WINDOWS EDITION") moves under the title when the top buttons need
  the room (it ran under them since the FloppaChat button).
- 3.3.2 (2026-11-25): LAUNCHER_VERSION comes from Cargo.toml (3.3.0/3.3.1 still said 3.2.0 and updated themselves in a loop); never a second self-update right after one.
- 3.4.0 (2026-11-25): FloppaChat 1.3 "Check for Updates > Update now": FloppaChat sends "chat" to the launcher port (or starts it with --open=chat) and closes; the launcher waits until it is closed (15 s at most), updates it and opens it again.
- 3.4.1 (2026-11-25): the FloppaChat button shows FloppaChat's own icon (rust/assets/chat.png, a copy of floppachat desktop/assets/icon64.png) instead of the 💬 sign.
- 3.5.0 (2026-11-25): self-update in two steps. Right after the start (first minute, nothing running) it updates and restarts by itself as before. Found later, or right after an update: it downloads the new launcher next to itself (<exe>.new, checked) and shows a gold "Launcher X is ready: click to restart and update"; closing the launcher puts it in place too (the next start is the new one). No automatic second update in a row, so a wrong version number can never loop again.

## Windows build: MSVC in GitHub Actions (since 3.5.1, 2026-11-25)
The Windows launcher is **no longer cross-built here with mingw** (3.4.1 from mingw was flagged "Virus detected" by
Windows Defender). A push that changes `rust/` runs `.github/workflows/launcher-windows.yml` on a Windows machine:
`cargo build --release --locked` with MSVC and a static C runtime (`rust/.cargo/config.toml`), checks the version
info (KianFloppa, version = Cargo.toml) and commits `builds/FloppaLauncher-<version>.zip`. Free: this repo is public.
Launcher release now: bump `version` in `rust/Cargo.toml`, push, wait for the workflow (about 5-8 min), `git pull`,
then the site's `tools/launcher.json` (url, size, sha256 of that zip) as before. Build the Linux zip here as before.
Never push a Windows zip from a session: the workflow skips a version whose zip already exists.

### Code signing (researched 2026-11-25, nothing bought; Kian decides)
- Microsoft's own service is **Artifact Signing** (renamed from Trusted Signing). Basic plan **$9.99 a month** for up to
  5,000 signatures (azure.microsoft.com/products/trusted-signing, checked 2026-11-25). Billing starts when the account
  is created. It needs an Azure subscription and an identity check; **who may sign up (individuals vs. organisations,
  which countries) was not confirmed** in this check and must be read on Microsoft's page before deciding.
- Other route: a classic code-signing certificate from a certificate authority, paid yearly, on a hardware key.
- False positives: submit the flagged file as "Software developer" at https://www.microsoft.com/en-us/wdsi/filesubmission.
  Microsoft has no allow-list program; reputation builds from telemetry, and each new version is new to it
  (learn.microsoft.com/defender-xdr/developer-faq). Submit every new launcher version that gets flagged.
