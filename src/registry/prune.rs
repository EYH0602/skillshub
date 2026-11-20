//! `skillshub prune` — uninstall installed skills whose sources no longer
//! list them, without updating any other installed skill.
//!
//! Unlike `update --prune`, nothing is refreshed or reinstalled: the command
//! only checks membership and removes confirmed orphans. Sources it cannot
//! verify (failed pull/scan/fetch) keep their skills and are reported, so a
//! stale cache or a network failure never deletes anything.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use colored::Colorize;

use super::db;
use super::github::{discover_skills_from_gist, fetch_gist, is_safe_skill_name, GistResponse};
use super::models::{Database, SkillId};
use super::skill::remove_installed_skill_files;
use super::tap::{ensure_fresh_clone, walk_local_skills};
use crate::paths::get_skills_install_dir;
use crate::skill::parse_frontmatter;

/// Where an installed skill comes from, for source grouping.
///
/// One source is checked once regardless of how many installed skills point
/// at it. `Git` is keyed by tap name — the default tap included, whose
/// membership is verified against its upstream clone like any other tap; the
/// bundled copy is an install source, not an upstream authority. `Gist` is
/// keyed by gist id, because the synthetic `<owner>/gists` tap can hold
/// skills from several gists.
#[derive(Debug, Ord, PartialOrd, PartialEq, Eq, Clone)]
enum Source {
    /// A clone-backed Git tap, keyed by tap name.
    Git(String),
    /// A GitHub gist, keyed by gist id.
    Gist(String),
}

impl Source {
    /// Human-readable name for reports.
    fn label(&self) -> String {
        match self {
            Source::Git(tap) => tap.clone(),
            Source::Gist(id) => format!("gist {}", id),
        }
    }
}

/// Uninstall installed skills that their sources no longer list.
///
/// With `tap_scope`, only installs from that tap are checked (the tap must
/// exist in the database). With `dry_run`, source data may still be fetched,
/// but installed files, agent links, and the database are left untouched.
///
/// Fails (nonzero exit) when any source check or removal failed, after all
/// other sources were still processed.
pub fn prune_skills(tap_scope: Option<&str>, dry_run: bool) -> Result<()> {
    let mut db = db::init_db()?;

    if let Some(name) = tap_scope {
        if !db.taps.contains_key(name) {
            bail!("Tap '{}' not found", name);
        }
    }

    // Group installed skills by the source to check. Sources without any
    // installed skill are never touched. Values are (full name, recorded
    // source path) so git checks can confirm presence by path.
    let mut by_source: BTreeMap<Source, Vec<(String, Option<String>)>> = BTreeMap::new();
    for (full_name, installed) in &db.installed {
        if let Some(scope) = tap_scope {
            if installed.tap != scope {
                continue;
            }
        }

        let source = if installed.gist_updated_at.is_some() {
            match &installed.source_path {
                // Gist installs store the gist id in `source_path`.
                Some(gist_id) => Source::Gist(gist_id.clone()),
                None => {
                    eprintln!(
                        "  {} {} kept (no gist id recorded; source unverifiable)",
                        "!".yellow(),
                        full_name
                    );
                    continue;
                }
            }
        } else if db.taps.contains_key(&installed.tap) {
            Source::Git(installed.tap.clone())
        } else {
            // A missing tap record is not proof of upstream deletion: these
            // are the skills `tap remove --keep-skills` intentionally keeps.
            eprintln!(
                "  {} {} kept (tap removed; source unverifiable)",
                "!".yellow(),
                full_name
            );
            continue;
        };
        by_source
            .entry(source)
            .or_default()
            .push((full_name.clone(), installed.source_path.clone()));
    }

    if by_source.is_empty() {
        match tap_scope {
            Some(name) => println!("No installed skills from tap '{}'.", name),
            None => println!("No skills installed; nothing to prune."),
        }
        return Ok(());
    }

    // Check every source; keep going when one fails.
    for names in by_source.values_mut() {
        names.sort();
    }
    let mut candidates: Vec<String> = Vec::new();
    let mut source_failures: usize = 0;
    for (source, skills) in &by_source {
        match check_source(&db, source) {
            Ok(upstream) => {
                for (name, source_path) in skills {
                    let skill = name.split('/').next_back().unwrap_or(name);
                    if !upstream.contains(skill, source_path) {
                        candidates.push(name.clone());
                    }
                }
            }
            Err(e) => {
                source_failures += 1;
                eprintln!(
                    "  {} {} (source check failed: {}); kept {} skill(s)",
                    "✗".red(),
                    source.label(),
                    e,
                    skills.len()
                );
            }
        }
    }

    candidates.sort();

    if candidates.is_empty() && source_failures == 0 {
        println!("{} No orphaned skills found.", "✓".green());
        return Ok(());
    }

    let mut removal_failures = 0usize;
    if !candidates.is_empty() {
        println!(
            "\n{} {} installed skill(s) no longer in their source:",
            "-".red().bold(),
            candidates.len()
        );
        for name in &candidates {
            println!("  {} {}", "-".red(), name);
        }

        if dry_run {
            println!("\n{} Dry run: no changes made.", "!".yellow().bold());
        } else {
            let install_dir = get_skills_install_dir()?;
            let mut removed = 0usize;
            for name in &candidates {
                match SkillId::parse(name) {
                    Some(skill_id) => match remove_installed_skill_files(&mut db, &install_dir, &skill_id) {
                        Ok(()) => removed += 1,
                        Err(e) => {
                            removal_failures += 1;
                            eprintln!("  {} failed to remove {}: {}", "✗".red(), name, e);
                        }
                    },
                    None => {
                        removal_failures += 1;
                        eprintln!("  {} failed to remove {}: invalid skill name", "✗".red(), name);
                    }
                }
            }
            if removed > 0 {
                db::save_db(&db)?;
                println!("\n{} Pruned {} skill(s)", "✓".green(), removed);
            }
        }
    }

    if source_failures > 0 || removal_failures > 0 {
        let mut reasons = Vec::new();
        if source_failures > 0 {
            reasons.push(format!("{} source check(s) failed", source_failures));
        }
        if removal_failures > 0 {
            reasons.push(format!("{} removal(s) failed", removal_failures));
        }
        bail!(
            "prune incomplete: {}; run again once the failures are resolved",
            reasons.join(", ")
        );
    }

    Ok(())
}

/// What a source check learned about upstream membership.
enum UpstreamSkills {
    /// Gists: membership is decided by discovered skill names only.
    Names(HashSet<String>),
    /// Git taps: scanned skill names plus the fresh clone, so presence can
    /// also be confirmed by the skill's recorded repo path.
    Clone { names: HashSet<String>, clone_dir: PathBuf },
}

impl UpstreamSkills {
    /// Whether an installed skill still exists upstream.
    ///
    /// For git taps a skill is present when the scan lists its name OR its
    /// recorded `source_path` still holds a `SKILL.md`. The path check is the
    /// identity anchor for installs whose recorded name is not a registry
    /// name (`skillshub add <url>` names skills after the URL folder, and a
    /// broken upstream frontmatter would hide a directory from the scan) —
    /// skipping those directories in the scan must not read as deletion.
    fn contains(&self, skill: &str, source_path: &Option<String>) -> bool {
        match self {
            UpstreamSkills::Names(names) => names.contains(skill),
            UpstreamSkills::Clone { names, clone_dir } => {
                names.contains(skill) || source_in_clone(clone_dir, source_path)
            }
        }
    }
}

/// Whether `source_path` (a repo-relative path recorded at install time)
/// currently holds a SKILL.md inside the fresh clone.
fn source_in_clone(clone_dir: &Path, source_path: &Option<String>) -> bool {
    let Some(path) = source_path else { return false };
    let rel = Path::new(path);
    // The path comes from our own install records, but the database is
    // user-editable JSON: never let it escape the clone.
    if rel.is_absolute() || rel.components().any(|c| c == std::path::Component::ParentDir) {
        return false;
    }
    clone_dir.join(rel).join("SKILL.md").is_file()
}

/// Collect the skill names a gist currently lists.
///
/// Files the API returned without content make the gist unverifiable: names
/// missing for that reason are not proof of deletion. So are ambiguity
/// signals — a `SKILL.md` that fails to parse or carries an unsafe name, and
/// any frontmatter-shaped file (starting with `---`) that fails to parse.
/// Plain files without frontmatter, such as a README, are ordinary gist
/// content and do not affect the verdict.
fn gist_skill_names(gist: &GistResponse) -> Result<HashSet<String>> {
    if gist.files.values().any(|f| f.content.is_none()) {
        bail!("file content missing from the API response (truncated?)");
    }
    for (filename, file) in &gist.files {
        let content = file.content.as_deref().unwrap_or_default();
        let is_skill_md = filename == "SKILL.md";
        if is_skill_md || content.starts_with("---") {
            match parse_frontmatter(content) {
                Ok(parsed) => {
                    if !is_safe_skill_name(&parsed.metadata.name) {
                        bail!(
                            "'{}' declares an unsafe skill name '{}'",
                            filename,
                            parsed.metadata.name
                        );
                    }
                }
                Err(e) => bail!(
                    "'{}' looks like a skill file but its frontmatter does not parse: {}",
                    filename,
                    e
                ),
            }
        }
    }
    Ok(discover_skills_from_gist(gist)
        .into_iter()
        .map(|(name, _)| name)
        .collect())
}

/// Collect the skill names a source currently lists.
///
/// `Ok(set)` is authoritative — an empty set means the source is confirmed
/// empty and every installed skill from it is an orphan. `Err` leaves the
/// decision to the caller: an unverifiable source must keep its skills.
fn check_source(db: &Database, source: &Source) -> Result<UpstreamSkills> {
    match source {
        Source::Gist(gist_id) => {
            // Fetched even when the recorded timestamp is unchanged: the
            // timestamp only tells us content changed, never that skills were
            // removed.
            let gist = fetch_gist(gist_id).with_context(|| format!("Failed to fetch gist {}", gist_id))?;
            let names = gist_skill_names(&gist).with_context(|| format!("gist {} could not be verified", gist_id))?;
            Ok(UpstreamSkills::Names(names))
        }

        Source::Git(tap_name) => {
            let tap = db
                .taps
                .get(tap_name)
                .with_context(|| format!("Tap '{}' not found", tap_name))?;

            // Refresh the configured branch once per tap — same refresh step
            // the update flows use, but without their bail-on-empty scan, so
            // a tap that legitimately has zero skills stays verifiable.
            let clone_dir = ensure_fresh_clone(tap_name, tap)?;

            // A complete scan with no skills is a valid empty source; an
            // interrupted scan returns Err and keeps every installed skill.
            let names = walk_local_skills(&clone_dir)?.into_keys().collect();
            Ok(UpstreamSkills::Clone { names, clone_dir })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::models::{InstalledSkill, TapInfo};
    use chrono::Utc;
    use serial_test::serial;
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    const TAP_A: &str = "owner-alpha/repo";
    const TAP_B: &str = "owner-beta/repo";

    /// RAII guard that restores `SKILLSHUB_TEST_HOME` on drop
    struct TestHomeGuard(Option<String>);

    impl TestHomeGuard {
        fn set(home: &Path) -> Self {
            let prev = std::env::var("SKILLSHUB_TEST_HOME").ok();
            std::env::set_var("SKILLSHUB_TEST_HOME", home);
            Self(prev)
        }
    }

    impl Drop for TestHomeGuard {
        fn drop(&mut self) {
            match self.0.take() {
                Some(v) => std::env::set_var("SKILLSHUB_TEST_HOME", v),
                None => std::env::remove_var("SKILLSHUB_TEST_HOME"),
            }
        }
    }

    /// Create a local git repo at `dir` with `skills/<name>/SKILL.md` per name.
    /// An empty `skills` list yields a valid but skill-less source.
    fn init_upstream(dir: &Path, skills: &[&str]) {
        fs::create_dir_all(dir).unwrap();
        let git = |args: &[&str]| {
            let out = Command::new("git").args(args).current_dir(dir).output().unwrap();
            assert!(
                out.status.success(),
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&out.stderr)
            );
        };
        git(&["init"]);
        git(&["config", "user.email", "test@test.com"]);
        git(&["config", "user.name", "Test"]);
        fs::write(dir.join("README.md"), "# test upstream").unwrap();
        for skill in skills {
            let skill_dir = dir.join("skills").join(skill);
            fs::create_dir_all(&skill_dir).unwrap();
            fs::write(
                skill_dir.join("SKILL.md"),
                format!("---\nname: {skill}\ndescription: test skill {}\n---\n\nbody", skill),
            )
            .unwrap();
        }
        git(&["add", "."]);
        git(&["commit", "-m", "init"]);
    }

    fn tap_info(url: &str, is_default: bool) -> TapInfo {
        TapInfo {
            url: url.to_string(),
            skills_path: "skills".to_string(),
            updated_at: None,
            is_default,
            cached_registry: None,
            branch: None,
        }
    }

    fn installed_record(tap: &str, skill: &str, source_path: &str) -> InstalledSkill {
        InstalledSkill {
            tap: tap.to_string(),
            skill: skill.to_string(),
            commit: None,
            installed_at: Utc::now(),
            source_url: None,
            source_path: Some(source_path.to_string()),
            gist_updated_at: None,
        }
    }

    /// Write a database with the default tap plus `taps` (name, url) and
    /// `installed` (tap, skill, source path) entries.
    fn fixture_db(taps: &[(&str, &str)], installed: &[(&str, &str, &str)]) {
        let mut db = Database::default();
        db.taps.insert(
            db::DEFAULT_TAP_NAME.to_string(),
            tap_info("https://github.com/EYH0602/skillshub", true),
        );
        for (name, url) in taps {
            db.taps.insert(name.to_string(), tap_info(url, false));
        }
        for (tap, skill, source_path) in installed {
            let full_name = format!("{}/{}", tap, skill);
            db.installed
                .insert(full_name, installed_record(tap, skill, source_path));
        }
        db::save_db(&db).unwrap();
    }

    fn skill_state(home: &Path, tap: &str, skill: &str) -> (bool, bool) {
        let dir = home.join(".skillshub/skills").join(tap).join(skill);
        let on_disk = dir.join("SKILL.md").is_file();
        let content = fs::read_to_string(home.join(".skillshub/db.json")).unwrap();
        let db: serde_json::Value = serde_json::from_str(&content).unwrap();
        let in_db = db["installed"][format!("{tap}/{skill}")].is_object();
        (on_disk, in_db)
    }

    /// Symlink `home/<agent_dir>/<skill>` -> installed skill dir. Returns the link path.
    fn link_skill(home: &Path, agent_dir: &str, tap: &str, skill: &str) -> std::path::PathBuf {
        let agent_skills = home.join(agent_dir).join("skills");
        fs::create_dir_all(&agent_skills).unwrap();
        let target = home.join(".skillshub/skills").join(tap).join(skill);
        let link = agent_skills.join(skill);
        std::os::unix::fs::symlink(&target, &link).unwrap();
        link
    }

    /// Prune keeps confirmed orphans' siblings, their agent links, unrelated
    /// taps, and links that are not skillshub-managed.
    #[test]
    #[serial]
    fn test_prune_removes_orphans_and_preserves_the_rest() {
        use tempfile::TempDir;
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let upstream_a = temp.path().join("upstream-a");
        let upstream_b = temp.path().join("upstream-b");
        init_upstream(&upstream_a, &["alpha"]);
        init_upstream(&upstream_b, &["kept"]);

        let _guard = TestHomeGuard::set(&home);
        fixture_db(
            &[
                (TAP_A, upstream_a.to_str().unwrap()),
                (TAP_B, upstream_b.to_str().unwrap()),
            ],
            &[
                (TAP_A, "alpha", "skills/alpha"),
                (TAP_A, "beta", "skills/beta"),
                (TAP_B, "kept", "skills/kept"),
            ],
        );
        for (tap, skill) in [(TAP_A, "alpha"), (TAP_A, "beta"), (TAP_B, "kept")] {
            let dir = home.join(".skillshub/skills").join(tap).join(skill);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("SKILL.md"), "installed copy").unwrap();
        }
        let beta_link = link_skill(&home, ".claude", TAP_A, "beta");
        let alpha_link = link_skill(&home, ".claude", TAP_A, "alpha");
        // A non-skillshub link inside the same agent dir must survive prune.
        let external_link = {
            let agent_skills = home.join(".claude").join("skills");
            let target = temp.path().join("outside-target");
            fs::create_dir_all(&target).unwrap();
            let link = agent_skills.join("external");
            std::os::unix::fs::symlink(&target, &link).unwrap();
            link
        };

        prune_skills(None, false).expect("prune should succeed");

        // Orphan gone from disk, db, and agent links.
        assert_eq!(
            skill_state(&home, TAP_A, "beta"),
            (false, false),
            "orphan should be pruned"
        );
        assert!(
            beta_link.symlink_metadata().is_err(),
            "managed link to pruned skill should go"
        );
        // Sibling and other tap untouched, links intact.
        assert_eq!(skill_state(&home, TAP_A, "alpha"), (true, true));
        assert_eq!(skill_state(&home, TAP_B, "kept"), (true, true));
        assert!(alpha_link.symlink_metadata().is_ok());
        assert!(external_link.symlink_metadata().is_ok(), "external link must survive");
    }

    /// Dry run checks sources but leaves files, links, and the database byte-identical.
    #[test]
    #[serial]
    fn test_prune_dry_run_changes_nothing() {
        use tempfile::TempDir;
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let upstream = temp.path().join("upstream");
        init_upstream(&upstream, &["alpha"]);

        let _guard = TestHomeGuard::set(&home);
        fixture_db(
            &[(TAP_A, upstream.to_str().unwrap())],
            &[(TAP_A, "alpha", "skills/alpha"), (TAP_A, "beta", "skills/beta")],
        );
        for skill in ["alpha", "beta"] {
            let dir = home.join(".skillshub/skills").join(TAP_A).join(skill);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("SKILL.md"), "installed copy").unwrap();
        }
        let beta_link = link_skill(&home, ".codex", TAP_A, "beta");
        let db_before = fs::read_to_string(home.join(".skillshub/db.json")).unwrap();

        prune_skills(None, true).expect("dry run should succeed");

        assert_eq!(
            skill_state(&home, TAP_A, "beta"),
            (true, true),
            "dry run must keep the orphan"
        );
        assert!(beta_link.symlink_metadata().is_ok(), "dry run must keep links");
        let db_after = fs::read_to_string(home.join(".skillshub/db.json")).unwrap();
        assert_eq!(db_before, db_after, "dry run must not write the database");
    }

    /// A confirmed empty source (complete scan, zero skills) authorizes removal;
    /// pruning the last skill also cleans the empty tap directory.
    #[test]
    #[serial]
    fn test_prune_last_skill_of_emptied_tap() {
        use tempfile::TempDir;
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let upstream = temp.path().join("upstream");
        init_upstream(&upstream, &[]);

        let _guard = TestHomeGuard::set(&home);
        fixture_db(
            &[(TAP_A, upstream.to_str().unwrap())],
            &[(TAP_A, "last-one", "skills/last-one")],
        );
        let dir = home.join(".skillshub/skills").join(TAP_A).join("last-one");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), "installed copy").unwrap();

        prune_skills(None, false).expect("empty source is a valid, complete scan");

        assert_eq!(skill_state(&home, TAP_A, "last-one"), (false, false));
        assert!(
            !home.join(".skillshub/skills").join(TAP_A).exists(),
            "empty tap directory should be cleaned up"
        );
    }

    /// A source that cannot be checked keeps its skills and fails the run.
    #[test]
    #[serial]
    fn test_prune_failed_source_keeps_skills() {
        use tempfile::TempDir;
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let missing = temp.path().join("does-not-exist");

        let _guard = TestHomeGuard::set(&home);
        fixture_db(&[(TAP_A, missing.to_str().unwrap())], &[(TAP_A, "beta", "skills/beta")]);
        let dir = home.join(".skillshub/skills").join(TAP_A).join("beta");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), "installed copy").unwrap();

        let result = prune_skills(None, false);
        assert!(result.is_err(), "unverifiable source must fail the run");
        assert_eq!(
            skill_state(&home, TAP_A, "beta"),
            (true, true),
            "failed check must keep the skill"
        );
    }

    /// Skills kept by `tap remove --keep-skills` (no tap record) are not orphans.
    #[test]
    #[serial]
    fn test_prune_keeps_skills_from_removed_tap() {
        use tempfile::TempDir;
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();

        let _guard = TestHomeGuard::set(&home);
        // No tap record for TAP_A: the skill was kept when the tap was removed.
        fixture_db(&[], &[(TAP_A, "retained", "skills/retained")]);
        let dir = home.join(".skillshub/skills").join(TAP_A).join("retained");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), "installed copy").unwrap();

        prune_skills(None, false).expect("kept skills are not an error");

        assert_eq!(
            skill_state(&home, TAP_A, "retained"),
            (true, true),
            "removed-tap skill must survive"
        );
    }

    /// Scoping checks only the named tap; unknown taps are rejected.
    #[test]
    #[serial]
    fn test_prune_scopes_to_one_tap() {
        use tempfile::TempDir;
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let upstream_a = temp.path().join("upstream-a");
        let upstream_b = temp.path().join("upstream-b");
        init_upstream(&upstream_a, &["alpha"]);
        init_upstream(&upstream_b, &["beta"]);

        let _guard = TestHomeGuard::set(&home);
        fixture_db(
            &[
                (TAP_A, upstream_a.to_str().unwrap()),
                (TAP_B, upstream_b.to_str().unwrap()),
            ],
            &[
                (TAP_A, "alpha", "skills/alpha"),
                (TAP_A, "orphan", "skills/orphan"),
                (TAP_B, "beta", "skills/beta"),
            ],
        );
        for (tap, skill) in [(TAP_A, "alpha"), (TAP_A, "orphan"), (TAP_B, "beta")] {
            let dir = home.join(".skillshub/skills").join(tap).join(skill);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("SKILL.md"), "installed copy").unwrap();
        }

        // Scoped to tap B: nothing pruned anywhere (B has no orphan).
        prune_skills(Some(TAP_B), false).expect("scoped prune should succeed");
        assert_eq!(
            skill_state(&home, TAP_A, "orphan"),
            (true, true),
            "unscoped tap must not be checked"
        );

        // Scoped to tap A: only A's orphan goes.
        prune_skills(Some(TAP_A), false).expect("scoped prune should succeed");
        assert_eq!(skill_state(&home, TAP_A, "orphan"), (false, false));
        assert_eq!(skill_state(&home, TAP_A, "alpha"), (true, true));
        assert_eq!(skill_state(&home, TAP_B, "beta"), (true, true), "other tap untouched");

        // Unknown tap name is rejected.
        assert!(prune_skills(Some("nope/repo"), false).is_err());
    }

    /// Regression: upstream frontmatter that fails to parse hides the name
    /// from the scan, but the skill's directory still holds a SKILL.md — the
    /// install must survive.
    #[test]
    #[serial]
    fn test_prune_keeps_skill_with_broken_upstream_frontmatter() {
        use tempfile::TempDir;
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let upstream = temp.path().join("upstream");
        // `init_upstream` writes valid frontmatter; break it afterwards.
        init_upstream(&upstream, &["foo"]);
        fs::write(upstream.join("skills/foo/SKILL.md"), "not frontmatter at all").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(&upstream)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-qm", "break frontmatter"])
            .current_dir(&upstream)
            .output()
            .unwrap();

        let _guard = TestHomeGuard::set(&home);
        fixture_db(&[(TAP_A, upstream.to_str().unwrap())], &[(TAP_A, "foo", "skills/foo")]);
        let dir = home.join(".skillshub/skills").join(TAP_A).join("foo");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), "installed copy").unwrap();

        prune_skills(None, false).expect("a broken upstream file is not a deletion");

        assert_eq!(
            skill_state(&home, TAP_A, "foo"),
            (true, true),
            "broken upstream frontmatter must keep the skill"
        );
    }

    /// Regression: `skillshub add <url>` names installs after the URL folder,
    /// so the scan (keyed by frontmatter name) lists "foo-skill" while the
    /// install is named "foo". Path presence must keep it.
    #[test]
    #[serial]
    fn test_prune_keeps_url_install_named_by_folder() {
        use tempfile::TempDir;
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let upstream = temp.path().join("upstream");
        init_upstream(&upstream, &[]);
        let foo_dir = upstream.join("skills/foo");
        fs::create_dir_all(&foo_dir).unwrap();
        fs::write(
            foo_dir.join("SKILL.md"),
            "---\nname: foo-skill\ndescription: different name than folder\n---\nbody",
        )
        .unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(&upstream)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-qm", "add foo"])
            .current_dir(&upstream)
            .output()
            .unwrap();

        let _guard = TestHomeGuard::set(&home);
        fixture_db(&[(TAP_A, upstream.to_str().unwrap())], &[(TAP_A, "foo", "skills/foo")]);
        let dir = home.join(".skillshub/skills").join(TAP_A).join("foo");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), "installed copy").unwrap();

        prune_skills(None, false).expect("folder-named install still exists upstream");

        assert_eq!(
            skill_state(&home, TAP_A, "foo"),
            (true, true),
            "path presence must keep the install"
        );
    }

    /// Gist ambiguity signals keep the gist unverifiable: content the API did
    /// not return, a SKILL.md that fails to parse, a frontmatter-shaped file
    /// that fails to parse, or an unsafe name. A plain README is ordinary
    /// gist content and does not block verification.
    #[test]
    fn test_gist_skill_names_rejects_ambiguity() {
        use crate::registry::github::{GistFile, GistOwner, GistResponse};
        use std::collections::HashMap;

        let make_gist = |files: &[(&str, Option<&str>)]| GistResponse {
            id: "abc123".to_string(),
            owner: GistOwner {
                login: "someone".to_string(),
            },
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            files: files
                .iter()
                .map(|(name, content)| {
                    (
                        name.to_string(),
                        GistFile {
                            filename: name.to_string(),
                            content: content.map(|c| c.to_string()),
                        },
                    )
                })
                .collect::<HashMap<_, _>>(),
        };

        let valid = "---\nname: gist-skill\ndescription: fine\n---\nbody";

        // Complete gist verifies and yields the discovered name.
        let names = gist_skill_names(&make_gist(&[("SKILL.md", Some(valid))])).expect("complete gist should verify");
        assert!(names.contains("gist-skill"));

        // A README without frontmatter is ordinary content, not ambiguity.
        let names = gist_skill_names(&make_gist(&[
            ("SKILL.md", Some(valid)),
            ("README.md", Some("# hello\n")),
        ]))
        .expect("a plain README must not block verification");
        assert!(names.contains("gist-skill"));

        // Content the API did not return.
        assert!(
            gist_skill_names(&make_gist(&[("SKILL.md", None)])).is_err(),
            "missing gist content must not verify names"
        );

        // A SKILL.md whose frontmatter does not parse is ambiguous.
        assert!(
            gist_skill_names(&make_gist(&[("SKILL.md", Some("broken file"))])).is_err(),
            "unparseable SKILL.md must not verify names"
        );

        // Any frontmatter-shaped file that fails to parse is ambiguous.
        assert!(
            gist_skill_names(&make_gist(&[
                ("SKILL.md", Some(valid)),
                ("notes.md", Some("---\nbroken"))
            ]))
            .is_err(),
            "unparseable frontmatter-shaped file must not verify names"
        );

        // An unsafe skill name in SKILL.md is ambiguous.
        assert!(
            gist_skill_names(&make_gist(&[(
                "SKILL.md",
                Some("---\nname: ../evil\ndescription: x\n---\n")
            )]))
            .is_err(),
            "unsafe skill name must not verify names"
        );
    }

    /// An interrupted scan — a SKILL.md that cannot be read mid-walk — must
    /// return Err, never a silently-truncated Ok. The readable sibling skill
    /// makes the regression real: the pre-refactor code silently skipped the
    /// unreadable file and returned Ok containing only the sibling.
    #[test]
    #[cfg(unix)]
    fn test_walk_local_skills_fails_on_unreadable_skill_md() {
        use std::os::unix::fs::PermissionsExt;
        use tempfile::TempDir;
        let temp = TempDir::new().unwrap();
        for (skill, body) in [
            ("alpha", "---\nname: alpha\ndescription: x\n---\n"),
            ("beta", "---\nname: beta\ndescription: y\n---\n"),
        ] {
            let dir = temp.path().join("skills").join(skill);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("SKILL.md"), body).unwrap();
        }
        let skill_md = temp.path().join("skills/alpha/SKILL.md");

        let unreadable = fs::Permissions::from_mode(0o000);
        let readable = fs::Permissions::from_mode(0o644);
        fs::set_permissions(&skill_md, unreadable).unwrap();
        // If we can still read it (e.g. running as root), the scenario is untestable here.
        if fs::read(&skill_md).is_ok() {
            fs::set_permissions(&skill_md, readable).unwrap();
            return;
        }

        let walked = walk_local_skills(temp.path());
        let discovered = super::super::tap::discover_skills_from_local(temp.path(), "test/tap");
        fs::set_permissions(&skill_md, readable).unwrap();

        assert!(walked.is_err(), "unreadable SKILL.md must interrupt the scan");
        assert!(discovered.is_err(), "discover must not silently skip unreadable skills");
    }

    /// walk_local_skills reports a complete-but-empty scan as Ok, while
    /// discover_skills_from_local still bails on it (update flows' behavior).
    #[test]
    fn test_walk_local_skills_empty_is_complete() {
        use tempfile::TempDir;
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("README.md"), "# no skills").unwrap();

        let walked = walk_local_skills(temp.path()).expect("empty walk should complete");
        assert!(walked.is_empty(), "no SKILL.md files means an empty source");

        let discovered = super::super::tap::discover_skills_from_local(temp.path(), "test/tap");
        assert!(discovered.is_err(), "discover must keep bailing on an empty clone");
    }
}
