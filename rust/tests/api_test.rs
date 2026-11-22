// Needs the game server running locally: (floppatreasure-game/server) npx wrangler dev --port 8799 --local
#[path = "../src/core.rs"]
#[allow(dead_code)]
mod core;

#[test]
fn signup_login_me_logout() {
    let base = "http://127.0.0.1:8799";
    let name = format!("LTest{}", std::process::id() % 100000);
    let (c, d) = core::api("POST", &format!("{base}/v1/signup"), Some(serde_json::json!({"name": name, "password": "good-pass-123x"})), None);
    assert_eq!(c, 200, "signup {d}");
    assert!(d["recovery"].as_str().unwrap_or("").len() > 10);
    let (c, d) = core::api("POST", &format!("{base}/v1/login"), Some(serde_json::json!({"name": name, "password": "good-pass-123x"})), None);
    assert_eq!(c, 200, "login {d}");
    let (id, tok) = (d["id"].as_str().unwrap().to_string(), d["token"].as_str().unwrap().to_string());
    let (c, d) = core::api("GET", &format!("{base}/v1/me"), None, Some((&id, &tok)));
    assert_eq!(c, 200);
    assert_eq!(d["name"].as_str().unwrap(), name);
    let (c, d) = core::api("POST", &format!("{base}/v1/login"), Some(serde_json::json!({"name": name, "password": "wrong-pass-123"})), None);
    assert_eq!(c, 401, "{d}");
    let (c, _) = core::api("POST", &format!("{base}/v1/logout"), None, Some((&id, &tok)));
    assert_eq!(c, 200);
    let (c, _) = core::api("GET", &format!("{base}/v1/me"), None, Some((&id, &tok)));
    assert_eq!(c, 401);
}

#[test]
fn godot_config_roundtrip() {
    let p = std::env::temp_dir().join("fl_cfg_test.cfg");
    std::fs::write(&p, "[account]\n\nid=\"abc\"\ntoken=\"t\\\"x\"\n\n[lobby]\n\nalone=true\n").unwrap();
    let mut c = core::Cfg::load(p.clone());
    assert_eq!(c.get_str("account", "id", ""), "abc");
    assert_eq!(c.get_str("account", "token", ""), "t\"x");
    assert!(c.get_bool("lobby", "alone", false));
    c.set_str("account", "id", "new");
    c.save();
    let c2 = core::Cfg::load(p);
    assert_eq!(c2.get_str("account", "id", ""), "new");
    assert!(c2.get_bool("lobby", "alone", false), "keys the launcher doesn't know are kept");
}

#[test]
fn versions() {
    assert!(core::newer("2.5", "2.4"));
    assert!(core::newer("3.0", "2.5"));
    assert!(!core::newer("2.5", "3.0"));
    assert_eq!(core::vname("1.6.11"), "Open Beta 1.6.11");
    assert_eq!(core::vname("2.0.0"), "Version 1.0");
    assert_eq!(core::vname("2.0.3"), "Version 1.0.3");
    assert_eq!(core::vname("2.1.0"), "Version 1.1");
    assert_eq!(core::vname("15.4.5"), "Prototype 15.4.5");
    assert_eq!(core::vname_for("felony", "122"), "Prototype 122");
    assert!(core::newer("2.0.0", "1.7.9") && !core::newer("1.7.9", "2.0.0"));
    assert_eq!(core::vname("116"), "Prototype 116");
}
