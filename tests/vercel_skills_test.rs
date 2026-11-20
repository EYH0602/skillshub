use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

fn cli(home: &Path, cwd: &Path, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_skillshub"))
        .env("SKILLSHUB_TEST_HOME", home)
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "{args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap()
}

fn skill(root: &Path, name: &str) -> PathBuf {
    let path = root.join(name);
    fs::create_dir_all(&path).unwrap();
    fs::write(
        path.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: Test skill\n---\n"),
    )
    .unwrap();
    path.canonicalize().unwrap()
}

#[test]
fn scan_finds_global_shared_store_without_detected_agents() {
    let home = TempDir::new().unwrap();
    let home = home.path();
    let shared = home.join(".agents/skills");
    skill(&shared, "shared-one");
    fs::create_dir(shared.join("not-a-skill")).unwrap();

    cli(home, home, &["external", "scan"]);
    let listed = cli(home, home, &["external", "list"]);
    assert!(
        listed.contains("shared-one") && listed.contains("~/.agents/skills"),
        "{listed}"
    );
    assert!(!listed.contains("not-a-skill"), "{listed}");
}

#[cfg(unix)]
#[test]
fn link_shares_live_sources_only_with_agents_that_need_them() {
    use std::os::unix::fs::symlink;

    let home = TempDir::new().unwrap();
    let home = home.path();
    let source = skill(&home.join(".agents/skills"), "shared-one");
    for agent in [".claude", ".codex", ".cursor", ".continue", ".augment"] {
        fs::create_dir_all(home.join(agent).join("skills")).unwrap();
    }
    let vercel_link = home.join(".continue/skills/shared-one");
    let relative_target = Path::new("../../.agents/skills/shared-one");
    symlink(relative_target, &vercel_link).unwrap();
    let dangling_link = home.join(".augment/skills/shared-one");
    symlink("not-installed", &dangling_link).unwrap();

    cli(home, home, &["link"]);
    let listed = cli(home, home, &["external", "list"]);
    assert!(
        listed.contains("shared-one") && listed.contains("~/.agents/skills"),
        "{listed}"
    );
    assert_eq!(fs::read_link(home.join(".claude/skills/shared-one")).unwrap(), source);
    for agent in [".codex", ".cursor"] {
        assert!(fs::symlink_metadata(home.join(agent).join("skills/shared-one")).is_err());
    }
    assert_eq!(fs::read_link(&vercel_link).unwrap(), relative_target);
    assert_eq!(fs::read_link(&dangling_link).unwrap(), Path::new("not-installed"));

    fs::remove_dir_all(source).unwrap();
    fs::create_dir(home.join(".openclaw")).unwrap();
    cli(home, home, &["link"]);
    assert!(fs::symlink_metadata(home.join(".openclaw/skills/shared-one")).is_err());
}

#[cfg(unix)]
#[test]
fn project_scan_reports_local_copies_without_promoting_or_migrating() {
    use std::os::unix::fs::symlink;

    let home = TempDir::new().unwrap();
    let home = home.path();
    let project = TempDir::new().unwrap();
    let project = project.path();
    let shared = project.join(".agents/skills");
    let project_skill = skill(&shared, "project-only");
    skill(&project.join(".kiro/skills"), "copied-for-kiro");
    let unrelated = skill(&home.join("unrelated"), "outside");
    symlink(&unrelated, shared.join("outside-link")).unwrap();
    symlink("no-such-directory", shared.join("broken-link")).unwrap();
    fs::create_dir(project.join(".cursor")).unwrap();
    symlink(&shared, project.join(".cursor/skills")).unwrap();
    symlink(home.join("unrelated"), project.join("skills")).unwrap();
    let legacy = skill(&home.join(".skillshub/skills"), "legacy");
    fs::create_dir(home.join(".claude")).unwrap();

    let output = cli(home, project, &["external", "scan", "--project", "."]);
    assert!(output.contains(".agents/skills/project-only"), "{output}");
    assert!(output.contains(".kiro/skills/copied-for-kiro"), "{output}");
    assert_eq!(output.matches("project-only").count(), 1, "{output}");
    assert!(
        !output.contains("outside") && !output.contains("broken-link"),
        "{output}"
    );
    assert!(legacy.join("SKILL.md").is_file());
    assert!(project_skill.join("SKILL.md").is_file());
    assert!(!home.join(".skillshub/db.json").exists());
    assert!(!home.join(".claude/skills").exists());
}

#[cfg(unix)]
#[test]
fn global_scan_does_not_adopt_a_redirected_shared_store() {
    use std::os::unix::fs::symlink;

    let home = TempDir::new().unwrap();
    let home = home.path();
    let project = TempDir::new().unwrap();
    let shared = project.path().join(".agents/skills");
    skill(&shared, "project-only");
    symlink(project.path().join(".agents"), home.join(".agents")).unwrap();

    cli(home, project.path(), &["external", "scan"]);
    let output = cli(home, project.path(), &["external", "list"]);
    assert!(!output.contains("project-only"), "{output}");
}
