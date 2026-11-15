extends SceneTree
## Screenshot + install test: godot --path . --script res://tests/shots.gd -- --site=http://127.0.0.1:8124 --out=/tmp/x

func _init() -> void:
	var out := "/tmp"
	for a in OS.get_cmdline_user_args():
		if a.begins_with("--out="): out = a.substr(6)
	var m: Control = load("res://main.tscn").instantiate()
	root.add_child(m)
	await create_timer(3.0).timeout
	print("status1: ", m.status.text, " | btn ", m.play_btn.text)
	root.get_texture().get_image().save_png(out + "/launcher-1.png")
	m._on_play()
	await create_timer(0.3).timeout
	root.get_texture().get_image().save_png(out + "/launcher-2.png")
	for i in 40:
		await create_timer(0.25).timeout
		if not m.busy: break
	print("status2: ", m.status.text, " | btn ", m.play_btn.text, " | installed ", m.installed)
	print("files: ", DirAccess.get_files_at("user://game"), DirAccess.get_directories_at("user://game"))
	root.get_texture().get_image().save_png(out + "/launcher-3.png")
	quit()
