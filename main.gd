extends Control
## Floppa Mining Launcher: installs, updates and starts the game. Shows the newest patch notes.
## The website is the source of truth:
##   https://floppamining.com/launcher/latest.json  {game, url, size, sha256, exe, launcher, launcher_url}
##   https://floppamining.com/launcher/news.json    [{title, date, v, url, image}]
## The game goes to <user dir>/game (Windows: %APPDATA%\FloppaMining\game).

const LAUNCHER_VERSION := "1.0"
var SITE := "https://floppamining.com"  # tests: -- --site=http://127.0.0.1:8124
const GOLD := Color("#ffd166")
const BG := Color("#0b0b0d")
const PANEL := Color(0.06, 0.06, 0.08, 0.86)
const GAME_DIR := "user://game"
const ZIP_PATH := "user://download.zip"
const CFG_PATH := "user://launcher.cfg"

var font: Font
var manifest := {}
var installed := ""
var busy := false
var cfg := ConfigFile.new()

var play_btn: Button
var status: Label
var bar: ProgressBar
var ver_label: Label
var news_box: HBoxContainer
var banner: Button
var dl: HTTPRequest
var dl_last := 0
var dl_speed := 0.0
var dl_t := 0.0


func _ready() -> void:
	for a in OS.get_cmdline_user_args():
		if a.begins_with("--site="):
			SITE = a.substr(7)
	font = load("res://fonts/Sniglet-Regular.ttf")
	cfg.load(CFG_PATH)
	installed = _read_installed()
	_build_ui()
	_refresh()
	_get_json(SITE + "/launcher/latest.json", _on_manifest)
	_get_json(SITE + "/launcher/news.json", _on_news)


# ---------- UI ----------

func _build_ui() -> void:
	var bg := ColorRect.new()
	bg.color = BG
	bg.set_anchors_and_offsets_preset(PRESET_FULL_RECT)
	add_child(bg)
	var pic := TextureRect.new()
	pic.texture = load("res://bg.png")
	pic.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	pic.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_COVERED
	pic.set_anchors_and_offsets_preset(PRESET_FULL_RECT)
	pic.modulate = Color(0.55, 0.55, 0.6)
	add_child(pic)
	var shade := TextureRect.new()
	var g := Gradient.new()
	g.set_color(0, Color(0, 0, 0, 0.15))
	g.set_color(1, Color(0, 0, 0, 0.92))
	var gt := GradientTexture2D.new()
	gt.gradient = g
	gt.fill_from = Vector2(0, 0)
	gt.fill_to = Vector2(0, 1)
	shade.texture = gt
	shade.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	shade.set_anchors_and_offsets_preset(PRESET_FULL_RECT)
	add_child(shade)

	# top bar
	var top := HBoxContainer.new()
	top.position = Vector2(32, 22)
	top.size = Vector2(1036, 50)
	top.add_theme_constant_override("separation", 18)
	add_child(top)
	var title := Label.new()
	title.text = "Floppa Mining"
	_style_label(title, 38, GOLD)
	title.add_theme_color_override("font_outline_color", Color(0, 0, 0, 0.8))
	title.add_theme_constant_override("outline_size", 8)
	top.add_child(title)
	var tag := Label.new()
	tag.text = "FREE TO PLAY"
	_style_label(tag, 14, Color(1, 1, 1, 0.7))
	tag.size_flags_vertical = SIZE_SHRINK_CENTER
	top.add_child(tag)
	var sp := Control.new()
	sp.size_flags_horizontal = SIZE_EXPAND_FILL
	top.add_child(sp)
	top.add_child(_link("Patch notes", SITE + "/updates/"))
	top.add_child(_link("Account", SITE + "/account/"))
	top.add_child(_link("Website", SITE + "/"))
	top.add_child(_settings_menu())

	# launcher update banner (hidden until the website says there is a newer launcher)
	banner = _button("A new launcher is out: click to download it", 15)
	banner.position = Vector2(32, 84)
	banner.size = Vector2(1036, 36)
	banner.visible = false
	banner.pressed.connect(func() -> void: OS.shell_open(str(manifest.get("launcher_page", SITE + "/download/"))))
	add_child(banner)

	# news
	var nl := Label.new()
	nl.text = "LATEST UPDATES"
	_style_label(nl, 15, GOLD)
	nl.position = Vector2(32, 300)
	add_child(nl)
	news_box = HBoxContainer.new()
	news_box.position = Vector2(32, 326)
	news_box.size = Vector2(1036, 176)
	news_box.add_theme_constant_override("separation", 16)
	add_child(news_box)
	var wait := Label.new()
	wait.text = "Loading patch notes..."
	_style_label(wait, 16, Color(1, 1, 1, 0.6))
	news_box.add_child(wait)

	# bottom bar
	var bottom := PanelContainer.new()
	bottom.add_theme_stylebox_override("panel", _box(PANEL, 0, 0))
	bottom.position = Vector2(0, 530)
	bottom.size = Vector2(1100, 110)
	add_child(bottom)
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 24)
	var m := MarginContainer.new()
	for side in ["left", "right", "top", "bottom"]:
		m.add_theme_constant_override("margin_" + side, 32 if side in ["left", "right"] else 20)
	m.add_child(row)
	bottom.add_child(m)
	var col := VBoxContainer.new()
	col.size_flags_horizontal = SIZE_EXPAND_FILL
	col.alignment = BoxContainer.ALIGNMENT_CENTER
	col.add_theme_constant_override("separation", 8)
	row.add_child(col)
	status = Label.new()
	_style_label(status, 18, Color.WHITE)
	col.add_child(status)
	bar = ProgressBar.new()
	bar.custom_minimum_size = Vector2(0, 12)
	bar.show_percentage = false
	bar.add_theme_stylebox_override("background", _box(Color(1, 1, 1, 0.1), 6, 0))
	bar.add_theme_stylebox_override("fill", _box(GOLD, 6, 0))
	bar.visible = false
	col.add_child(bar)
	ver_label = Label.new()
	_style_label(ver_label, 13, Color(1, 1, 1, 0.5))
	col.add_child(ver_label)
	play_btn = Button.new()
	play_btn.custom_minimum_size = Vector2(250, 66)
	play_btn.add_theme_font_override("font", font)
	play_btn.add_theme_font_size_override("font_size", 30)
	for st in ["normal", "hover", "pressed", "focus", "disabled"]:
		var c := GOLD
		if st == "hover": c = GOLD.lightened(0.15)
		elif st == "pressed": c = GOLD.darkened(0.15)
		elif st == "disabled": c = Color(0.35, 0.33, 0.3)
		play_btn.add_theme_stylebox_override(st, _box(c, 14, 0))
	for fc in ["font_color", "font_hover_color", "font_pressed_color", "font_focus_color"]:
		play_btn.add_theme_color_override(fc, Color("#1b1006"))
	play_btn.add_theme_color_override("font_disabled_color", Color(1, 1, 1, 0.5))
	play_btn.pressed.connect(_on_play)
	row.add_child(play_btn)


func _style_label(l: Label, size: int, c: Color) -> void:
	l.add_theme_font_override("font", font)
	l.add_theme_font_size_override("font_size", size)
	l.add_theme_color_override("font_color", c)


func _box(c: Color, r: int, border: int, bc := Color(1, 1, 1, 0.12)) -> StyleBoxFlat:
	var s := StyleBoxFlat.new()
	s.bg_color = c
	s.set_corner_radius_all(r)
	s.set_border_width_all(border)
	s.border_color = bc
	s.content_margin_left = 14
	s.content_margin_right = 14
	s.content_margin_top = 6
	s.content_margin_bottom = 6
	return s


func _button(text: String, size: int) -> Button:
	var b := Button.new()
	b.text = text
	b.add_theme_font_override("font", font)
	b.add_theme_font_size_override("font_size", size)
	b.add_theme_stylebox_override("normal", _box(Color(0, 0, 0, 0.55), 10, 1))
	b.add_theme_stylebox_override("hover", _box(Color(0.15, 0.13, 0.08, 0.8), 10, 1, GOLD))
	b.add_theme_stylebox_override("pressed", _box(Color(0.1, 0.1, 0.1, 0.9), 10, 1, GOLD))
	b.add_theme_stylebox_override("focus", StyleBoxEmpty.new())
	b.add_theme_color_override("font_hover_color", GOLD)
	b.mouse_default_cursor_shape = CURSOR_POINTING_HAND
	return b


func _link(text: String, url: String) -> Button:
	var b := _button(text, 17)
	b.size_flags_vertical = SIZE_SHRINK_CENTER
	b.pressed.connect(func() -> void: OS.shell_open(url))
	return b


func _settings_menu() -> MenuButton:
	var mb := MenuButton.new()
	mb.text = "⚙"
	mb.flat = false
	mb.add_theme_font_size_override("font_size", 20)
	mb.add_theme_stylebox_override("normal", _box(Color(0, 0, 0, 0.55), 10, 1))
	mb.add_theme_stylebox_override("hover", _box(Color(0.15, 0.13, 0.08, 0.8), 10, 1, GOLD))
	mb.add_theme_stylebox_override("pressed", _box(Color(0.1, 0.1, 0.1, 0.9), 10, 1, GOLD))
	mb.size_flags_vertical = SIZE_SHRINK_CENTER
	mb.tooltip_text = "Settings"
	var p := mb.get_popup()
	p.add_theme_font_override("font", font)
	p.add_theme_font_size_override("font_size", 16)
	p.add_check_item("Close the launcher when the game starts", 0)
	p.set_item_checked(0, bool(cfg.get_value("launcher", "close_on_play", true)))
	p.add_separator()
	p.add_item("Open the game folder", 1)
	p.add_item("Repair (download the game again)", 2)
	p.add_item("Uninstall the game", 3)
	p.add_separator()
	p.add_item("Launcher %s" % LAUNCHER_VERSION, 9)
	p.set_item_disabled(p.get_item_index(9), true)
	p.id_pressed.connect(_on_setting.bind(p))
	return mb


func _on_setting(id: int, p: PopupMenu) -> void:
	match id:
		0:
			var i := p.get_item_index(0)
			p.set_item_checked(i, not p.is_item_checked(i))
			cfg.set_value("launcher", "close_on_play", p.is_item_checked(i))
			cfg.save(CFG_PATH)
		1:
			DirAccess.make_dir_recursive_absolute(GAME_DIR)
			OS.shell_open(ProjectSettings.globalize_path(GAME_DIR))
		2:
			if not busy and not manifest.is_empty():
				_start_download()
		3:
			if not busy:
				_remove_dir(GAME_DIR)
				installed = ""
				_refresh()


# ---------- state ----------

func _read_installed() -> String:
	var f := FileAccess.open(GAME_DIR + "/version.txt", FileAccess.READ)
	if f == null:
		return ""
	if not FileAccess.file_exists(_exe_path()):
		return ""
	return f.get_as_text().strip_edges()


func _exe_path() -> String:
	return GAME_DIR + "/" + str(manifest.get("exe", cfg.get_value("launcher", "exe", "FloppaMining.exe")))


func _refresh() -> void:
	var online := str(manifest.get("game", ""))
	ver_label.text = "Installed: %s   ·   Newest: %s   ·   Launcher %s" % [
		("Prototype " + installed) if installed != "" else "nothing yet",
		("Prototype " + online) if online != "" else "checking...", LAUNCHER_VERSION]
	if busy:
		return
	play_btn.disabled = false
	if installed == "" and online == "":
		play_btn.text = "INSTALL"
		play_btn.disabled = true
		status.text = "Connecting to floppamining.com..."
	elif installed == "":
		play_btn.text = "INSTALL"
		status.text = "Ready to install (%s)" % _mb(int(manifest.get("size", 0)))
	elif online != "" and online != installed:
		play_btn.text = "UPDATE"
		status.text = "Prototype %s is out (%s)" % [online, _mb(int(manifest.get("size", 0)))]
	else:
		play_btn.text = "PLAY"
		status.text = "Ready to play" if online != "" else "Ready to play (couldn't check for updates)"
	banner.visible = manifest.has("launcher") and _newer(str(manifest.launcher), LAUNCHER_VERSION)


func _newer(a: String, b: String) -> bool:
	var pa := a.split("."); var pb := b.split(".")
	for i in maxi(pa.size(), pb.size()):
		var x := int(pa[i]) if i < pa.size() else 0
		var y := int(pb[i]) if i < pb.size() else 0
		if x != y:
			return x > y
	return false


func _mb(bytes: int) -> String:
	return "%.0f MB" % (bytes / 1048576.0) if bytes > 0 else "size unknown"


# ---------- network ----------

func _get_json(url: String, cb: Callable) -> void:
	var r := HTTPRequest.new()
	r.timeout = 15
	add_child(r)
	r.request_completed.connect(func(res: int, code: int, _h: PackedStringArray, body: PackedByteArray) -> void:
		r.queue_free()
		if res == HTTPRequest.RESULT_SUCCESS and code == 200:
			cb.call(JSON.parse_string(body.get_string_from_utf8()))
		else:
			cb.call(null))
	r.request(url + "?t=%d" % Time.get_unix_time_from_system())


func _on_manifest(d: Variant) -> void:
	if d is Dictionary and d.has("game") and d.has("url"):
		manifest = d
		cfg.set_value("launcher", "exe", str(d.get("exe", "FloppaMining.exe")))
		cfg.save(CFG_PATH)
		installed = _read_installed()
	elif installed == "":
		status.text = "Can't reach floppamining.com. Check your internet and restart the launcher."
	_refresh()


func _on_news(d: Variant) -> void:
	for c in news_box.get_children():
		c.queue_free()
	if not (d is Array) or d.is_empty():
		var l := Label.new()
		l.text = "Couldn't load the patch notes."
		_style_label(l, 16, Color(1, 1, 1, 0.6))
		news_box.add_child(l)
		return
	for item in d.slice(0, 3):
		news_box.add_child(_news_card(item))


func _news_card(item: Dictionary) -> Button:
	var card := Button.new()
	card.custom_minimum_size = Vector2(334, 176)
	card.add_theme_stylebox_override("normal", _box(PANEL, 14, 1))
	card.add_theme_stylebox_override("hover", _box(Color(0.12, 0.1, 0.06, 0.92), 14, 1, GOLD))
	card.add_theme_stylebox_override("pressed", _box(PANEL, 14, 1, GOLD))
	card.add_theme_stylebox_override("focus", StyleBoxEmpty.new())
	card.mouse_default_cursor_shape = CURSOR_POINTING_HAND
	card.clip_contents = true
	card.pressed.connect(func() -> void: OS.shell_open(SITE + str(item.get("url", "/updates/"))))
	var img := TextureRect.new()
	img.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	img.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_COVERED
	img.position = Vector2(1, 1)
	img.size = Vector2(332, 104)
	img.mouse_filter = MOUSE_FILTER_IGNORE
	card.add_child(img)
	var box := VBoxContainer.new()
	box.position = Vector2(14, 110)
	box.size = Vector2(306, 62)
	box.add_theme_constant_override("separation", 0)
	box.mouse_filter = MOUSE_FILTER_IGNORE
	card.add_child(box)
	var v := Label.new()
	v.text = "%s  ·  %s" % [str(item.get("v", "")), str(item.get("date", ""))]
	_style_label(v, 13, GOLD)
	v.mouse_filter = MOUSE_FILTER_IGNORE
	box.add_child(v)
	var t := Label.new()
	t.text = str(item.get("title", ""))
	_style_label(t, 16, Color.WHITE)
	t.custom_minimum_size = Vector2(306, 0)
	t.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	t.max_lines_visible = 2
	t.add_theme_constant_override("line_spacing", -3)
	t.mouse_filter = MOUSE_FILTER_IGNORE
	box.add_child(t)
	var u := str(item.get("image", ""))
	if u != "":
		var r := HTTPRequest.new()
		add_child(r)
		r.request_completed.connect(func(res: int, code: int, _h: PackedStringArray, body: PackedByteArray) -> void:
			r.queue_free()
			if res == HTTPRequest.RESULT_SUCCESS and code == 200 and is_instance_valid(img):
				var im := Image.new()
				if im.load_png_from_buffer(body) == OK:
					img.texture = ImageTexture.create_from_image(im))
		r.request(SITE + u)
	return card


# ---------- install / play ----------

func _on_play() -> void:
	if busy:
		return
	var online := str(manifest.get("game", ""))
	if installed != "" and (online == "" or online == installed):
		_launch()
	elif not manifest.is_empty():
		_start_download()


func _start_download() -> void:
	busy = true
	play_btn.disabled = true
	play_btn.text = "..."
	bar.visible = true
	bar.value = 0
	status.text = "Downloading Prototype %s..." % manifest.game
	if FileAccess.file_exists(ZIP_PATH):
		DirAccess.remove_absolute(ZIP_PATH)
	dl = HTTPRequest.new()
	dl.use_threads = true
	dl.download_file = ZIP_PATH
	dl.download_chunk_size = 262144
	dl.timeout = 0
	add_child(dl)
	dl.request_completed.connect(_on_downloaded)
	dl_last = 0
	dl_speed = 0.0
	dl_t = 0.0
	var err := dl.request(str(manifest.url))
	if err != OK:
		_fail("The download couldn't start (error %d)." % err)


func _process(delta: float) -> void:
	if dl == null or not is_instance_valid(dl):
		return
	var got := dl.get_downloaded_bytes()
	var total := int(manifest.get("size", 0))
	if total <= 0:
		total = dl.get_body_size()
	dl_t += delta
	if dl_t >= 0.5:
		dl_speed = lerpf(dl_speed, (got - dl_last) / dl_t, 0.5 if dl_speed > 0 else 1.0)
		dl_last = got
		dl_t = 0.0
	if total > 0:
		bar.value = 100.0 * got / total
		var left := ""
		if dl_speed > 1000:
			var s := int((total - got) / dl_speed)
			left = " · %d:%02d left" % [s / 60, s % 60]
		status.text = "Downloading Prototype %s: %s / %s · %.1f MB/s%s" % [manifest.game, _mb(got), _mb(total), dl_speed / 1048576.0, left]


func _on_downloaded(res: int, code: int, _h: PackedStringArray, _b: PackedByteArray) -> void:
	dl.queue_free()
	dl = null
	if res != HTTPRequest.RESULT_SUCCESS or code != 200:
		_fail("The download failed (%s). Check your internet and try again." % (("HTTP %d" % code) if code > 0 else "result %d" % res))
		return
	status.text = "Checking the download..."
	bar.value = 100
	await get_tree().process_frame
	var want := str(manifest.get("sha256", "")).to_lower()
	if want != "" and FileAccess.get_sha256(ZIP_PATH) != want:
		DirAccess.remove_absolute(ZIP_PATH)
		_fail("The download was damaged. Click to try again.")
		return
	status.text = "Installing..."
	await get_tree().process_frame
	var err := _extract(ZIP_PATH, "user://game_new")
	DirAccess.remove_absolute(ZIP_PATH)
	if err != "":
		_remove_dir("user://game_new")
		_fail(err)
		return
	_remove_dir(GAME_DIR)
	if DirAccess.rename_absolute("user://game_new", GAME_DIR) != OK:
		_fail("Couldn't finish installing. Is the game still open?")
		return
	var f := FileAccess.open(GAME_DIR + "/version.txt", FileAccess.WRITE)
	f.store_string(str(manifest.game))
	f.close()
	installed = _read_installed()
	busy = false
	bar.visible = false
	_refresh()
	status.text = "Prototype %s is installed. Have fun!" % installed


func _extract(zip: String, to: String) -> String:
	_remove_dir(to)
	DirAccess.make_dir_recursive_absolute(to)
	var z := ZIPReader.new()
	if z.open(zip) != OK:
		return "The download couldn't be opened."
	for p in z.get_files():
		if p.begins_with("/") or ".." in p:
			continue
		var out := to + "/" + p
		if p.ends_with("/"):
			DirAccess.make_dir_recursive_absolute(out)
			continue
		DirAccess.make_dir_recursive_absolute(out.get_base_dir())
		var w := FileAccess.open(out, FileAccess.WRITE)
		if w == null:
			z.close()
			return "Couldn't write the game files (is there enough space?)."
		w.store_buffer(z.read_file(p))
		w.close()
	z.close()
	return ""


func _remove_dir(path: String) -> void:
	var d := DirAccess.open(path)
	if d == null:
		return
	d.include_hidden = true
	for f in d.get_files():
		d.remove(f)
	for sub in d.get_directories():
		_remove_dir(path + "/" + sub)
	DirAccess.remove_absolute(path)


func _launch() -> void:
	var exe := ProjectSettings.globalize_path(_exe_path())
	var pid := OS.create_process(exe, [])
	if pid <= 0:
		status.text = "The game couldn't start. Try Settings → Repair."
		return
	status.text = "Starting Floppa Mining..."
	play_btn.disabled = true
	if bool(cfg.get_value("launcher", "close_on_play", true)):
		await get_tree().create_timer(1.5).timeout
		get_tree().quit()
	else:
		await get_tree().create_timer(4.0).timeout
		_refresh()


func _fail(msg: String) -> void:
	busy = false
	bar.visible = false
	_refresh()
	status.text = msg
