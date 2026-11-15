# Floppa Mining Launcher

Small launcher for **Floppa Mining** (free to play, Windows). It installs the game, updates it and shows the newest patch notes.
Download: https://floppamining.com/download/

- Made with Godot 4.8 (`main.gd` is the whole launcher).
- Reads https://floppamining.com/launcher/latest.json (game version, zip URL, size, sha256) and `/launcher/news.json`.
- Installs to `%APPDATA%\FloppaMining\game`, checks the sha256 of every download.
- The game and launcher builds are in `builds/` (GitHub Releases can't be created from the AI sessions). Delete builds older than the last 2 to keep the repo small.
  The game's source code is private; only the builds are here.

## Release a new game version (for AI sessions)
1. In the game repo: bump `config/version`, `godot --headless --path . --export-release "Windows" ../fmbuild/game/FloppaMining.exe`
2. Zip it: `FloppaMining-<v>.zip` with `FloppaMining.exe` at the root.
3. Put the zip in `builds/` of this repo, push.
4. In floppatreasure-site: update `tools/launcher.json` (game, url, size, sha256), `python3 tools/build.py`, push.
   The launcher shows UPDATE as soon as the site is live.

## Test
`godot --path . --script res://tests/shots.gd -- --site=http://127.0.0.1:8124 --out=/tmp/x` with a local copy of the launcher JSON files.
