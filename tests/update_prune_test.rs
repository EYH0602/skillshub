mod common;

use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::{json, Value};
use tempfile::TempDir;

fn update(home: &Path, prune: bool) -> String {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_skillshub"));
    cmd.env("SKILLSHUB_TEST_HOME", home).current_dir(home).arg("update");
    if prune {
        cmd.arg("--prune");
    }
    let result = cmd.output().unwrap();
    assert!(result.status.success(), "{result:?}");
    String::from_utf8(result.stdout).unwrap()
}

#[test]
fn default_tap_prunes_only_after_checking_upstream() {
    let temp = TempDir::new().unwrap();
    let home = temp.path();
    let upstream = home.join("upstream");
    let state = home.join(".skillshub");
    let db_path = state.join("db.json");
    let tap = "EYH0602/skillshub";
    let obsolete = format!("{tap}/write-unit-tests");
    let current = format!("{tap}/using-skillshub");
    let obsolete_dir = state.join("skills").join(&obsolete);
    let current_dir = state.join("skills").join(&current);
    for dir in [&obsolete_dir, &current_dir] {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("SKILL.md"), "old copy").unwrap();
    }
    let install_record = |skill: &str| {
        json!({"tap": tap, "skill": skill, "commit": null, "installed_at": "2026-01-01T00:00:00Z"})
    };
    let db = json!({
        "taps": {
            (tap): {
                "url": upstream,
                "skills_path": "skills",
                "is_default": true,
                "cached_registry": {
                    "name": tap,
                    "skills": {"write-unit-tests": {"path": "skills/write-unit-tests"}}
                }
            }
        },
        "installed": {
            (&obsolete): install_record("write-unit-tests"),
            (&current): install_record("using-skillshub")
        }
    });
    fs::write(&db_path, db.to_string()).unwrap();

    // The origin is unavailable. Neither the stale cache hit nor its miss is proof.
    let read_db = || serde_json::from_slice::<Value>(&fs::read(&db_path).unwrap()).unwrap();
    let output = update(home, true);
    assert!(output.contains("Failed to clone"), "{output}");
    let failed_db = read_db();
    for (name, dir) in [(&obsolete, &obsolete_dir), (&current, &current_dir)] {
        assert!(failed_db["installed"][name].is_object());
        assert_eq!(fs::read_to_string(dir.join("SKILL.md")).unwrap(), "old copy");
    }

    fs::create_dir(&upstream).unwrap();
    common::init_test_repo_with_skill(&upstream, "using-skillshub", "current upstream copy");

    let output = update(home, false);
    assert!(output.contains(&format!("{obsolete} (no longer in tap)")), "{output}");
    assert!(obsolete_dir.is_dir());
    assert!(read_db()["installed"][&obsolete].is_object());
    let current_doc = fs::read_to_string(current_dir.join("SKILL.md")).unwrap();
    assert!(current_doc.contains("current upstream copy"));

    let output = update(home, true);
    assert!(output.contains(&format!("{obsolete} (pruned, no longer in tap)")), "{output}");
    assert!(!obsolete_dir.exists());
    let final_db = read_db();
    let installed = final_db["installed"].as_object().unwrap();
    assert!(!installed.contains_key(&obsolete));
    assert!(installed.contains_key(&current));
    assert!(current_dir.join("SKILL.md").is_file());
}
