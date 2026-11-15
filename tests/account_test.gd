extends SceneTree
## Login test: godot --headless --path . --script res://tests/account_test.gd -- --api=http://127.0.0.1:8799 --site=http://127.0.0.1:1

func _init() -> void:
	var m: Control = load("res://main.tscn").instantiate()
	root.add_child(m)
	await create_timer(0.5).timeout
	m._save_game_login("", "")
	var r: Dictionary = await m._api(HTTPClient.METHOD_POST, "/v1/signup", {"name": "LT%d" % (Time.get_ticks_usec() % 100000000), "password": "floppa-launch-99"})
	print("signup ", r.code, " ", r.data.keys())
	m._save_game_login(str(r.data.id), str(r.data.token))
	m.acc_id = ""
	await m._load_account()
	print("name after reload: ", m.acc_name, " btn ", m.acc_btn.text)
	var cf := ConfigFile.new()
	print("cfg ", cf.load(m._game_cfg_path()), " ", m._game_cfg_path(), " id set ", cf.get_value("account", "id", "") != "")
	m._open_account()
	await process_frame
	print("running before: ", m._is_game_running())
	var lock := TCPServer.new()
	print("lock ", lock.listen(47731, "127.0.0.1"))
	print("running with lock: ", m._is_game_running())
	m._check_t = 0.0
	await create_timer(0.3).timeout
	print("btn while running: ", m.play_btn.text, " disabled ", m.play_btn.disabled)
	lock.stop()
	await m._api(HTTPClient.METHOD_POST, "/v1/logout", {})
	m._save_game_login("", "")
	quit()
