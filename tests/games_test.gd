extends SceneTree
## 2.5 two-game test: godot --path . --script res://tests/games_test.gd -- --site=http://127.0.0.1:8124 --out=/tmp/x
var fails := 0
func check(ok: bool, what: String) -> void:
	print(("OK   " if ok else "FAIL ") + what)
	if not ok: fails += 1

func _init() -> void:
	var out := "/tmp"
	for a in OS.get_cmdline_user_args():
		if a.begins_with("--out="): out = a.substr(6)
	var m: Control = load("res://main.tscn").instantiate()
	root.add_child(m)
	m._remove_dir("user://felony")
	await create_timer(3.0).timeout
	for i in 40:
		await create_timer(0.25).timeout
		if not m.busy: break
	check(m.cur == "mining" or m.cur == "felony", "starts on a known game (%s)" % m.cur)
	m._switch_game("mining")
	await create_timer(2.0).timeout
	for i in 40:
		await create_timer(0.25).timeout
		if not m.busy: break
	check(m.title_lbl.text == "Floppa Mining", "Mining shown")
	check(m.installed == "1.6.11", "Mining installed in user://game (%s)" % m.installed)
	root.get_texture().get_image().save_png(out + "/l-mining.png")
	m._switch_game("felony")
	await create_timer(1.5).timeout
	for i in 120:
		await create_timer(0.25).timeout
		if not m.busy: break
	check(m.title_lbl.text == "Floppa Felony", "Felony shown")
	check(m.installed == "116", "Felony installed (%s) status: %s" % [m.installed, m.status.text])
	check(FileAccess.file_exists("user://felony/FloppaFelony.exe") and FileAccess.file_exists("user://felony/FloppaFelony.pck"), "Felony files in user://felony")
	check(FileAccess.file_exists("user://game/FloppaMining.exe"), "Mining files untouched")
	check(m.play_btn.text == "PLAY", "PLAY button for Felony (%s)" % m.play_btn.text)
	check(str(m.site_link.get_meta("url")) == "https://felony.floppaland.com/", "Website link points to the Felony site")
	root.get_texture().get_image().save_png(out + "/l-felony.png")
	m._switch_game("mining")
	await create_timer(1.0).timeout
	check(m.installed == "1.6.11" and m.play_btn.text == "PLAY", "back to Mining: still installed, PLAY")
	print("RESULT: %d failures" % fails)
	quit(1 if fails else 0)
