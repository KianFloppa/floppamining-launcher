// The install folder (3.5.4, Linux): games go to "<picked>/FloppaGames/<game>", installed games move along, and a
// game already in the new place is left alone. Run: cargo test --release --test install_dir_test
#[path = "../src/core.rs"]
#[allow(dead_code)]
mod core;
use core::*;
use std::fs;

#[test]
fn install_folder() {
    let tmp = std::env::temp_dir().join(format!("fl_inst_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    let (old, picked) = (tmp.join("old"), tmp.join("picked"));
    // an installed game with a file that may be run, and a sub-folder
    fs::create_dir_all(old.join("game/sub")).unwrap();
    fs::write(old.join("game/FloppaMining.x86_64"), b"exe").unwrap();
    fs::write(old.join("game/version.txt"), b"2.5.1").unwrap();
    fs::write(old.join("game/sub/a.pck"), b"data").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(old.join("game/FloppaMining.x86_64"), fs::Permissions::from_mode(0o755)).unwrap();
    }
    // the picked folder already holds something of the player's
    fs::create_dir_all(&picked).unwrap();
    fs::write(picked.join("my_notes.txt"), b"keep me").unwrap();

    assert!(check_install_dir(std::path::Path::new("relative/folder")).is_err(), "a relative path is refused");
    assert!(check_install_dir(&picked).is_ok(), "a writable folder is fine");
    let to = picked.join(INSTALL_SUB);
    assert_eq!(move_installs(&old, &to).unwrap(), 1, "one game moved");
    assert!(to.join("game/FloppaMining.x86_64").exists() && to.join("game/sub/a.pck").exists(), "all files arrived");
    assert_eq!(fs::read_to_string(to.join("game/version.txt")).unwrap(), "2.5.1");
    assert!(!old.join("game").exists(), "the old copy is gone");
    assert!(picked.join("my_notes.txt").exists(), "the player's own file is untouched");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(to.join("game/FloppaMining.x86_64")).unwrap().permissions().mode() & 0o111, 0o111, "still allowed to run");
    }
    // game_dir follows the setting
    set_install_dir(picked.to_str().unwrap());
    assert_eq!(game_dir("mining"), to.join("game"));
    assert_eq!(installed_version("mining", "FloppaMining.x86_64"), "2.5.1");
    // a game already in the new place is not overwritten
    fs::create_dir_all(old.join("game")).unwrap();
    fs::write(old.join("game/version.txt"), b"1.0.0").unwrap();
    assert_eq!(move_installs(&old, &to).unwrap(), 0);
    assert_eq!(fs::read_to_string(to.join("game/version.txt")).unwrap(), "2.5.1");
    // back to the default
    set_install_dir("");
    assert_eq!(game_dir("mining"), data_dir().join("game"));
    let _ = fs::remove_dir_all(&tmp);
}
