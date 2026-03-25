//! Tests for shell completions subcommand
//!
//! Verifies that the completions subcommand exists and produces
//! non-empty, valid completion scripts for each supported shell.

use std::process::Command;

fn cargo_bin() -> Command {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.args(["run", "--quiet", "--"]);
    cmd
}

#[test]
fn test_completions_bash_produces_output() {
    let output = cargo_bin()
        .args(["completions", "bash"])
        .output()
        .expect("failed to run skillshub completions bash");

    assert!(output.status.success(), "command should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.is_empty(), "bash completions should not be empty");
    assert!(
        stdout.contains("_skillshub"),
        "bash completions should contain the completion function"
    );
}

#[test]
fn test_completions_zsh_produces_output() {
    let output = cargo_bin()
        .args(["completions", "zsh"])
        .output()
        .expect("failed to run skillshub completions zsh");

    assert!(output.status.success(), "command should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.is_empty(), "zsh completions should not be empty");
    assert!(
        stdout.contains("#compdef skillshub"),
        "zsh completions should contain compdef directive"
    );
}

#[test]
fn test_completions_fish_produces_output() {
    let output = cargo_bin()
        .args(["completions", "fish"])
        .output()
        .expect("failed to run skillshub completions fish");

    assert!(output.status.success(), "command should succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.is_empty(), "fish completions should not be empty");
    assert!(
        stdout.contains("complete -c skillshub"),
        "fish completions should contain complete command"
    );
}

#[test]
fn test_completions_invalid_shell_fails() {
    let output = cargo_bin()
        .args(["completions", "powershell"])
        .output()
        .expect("failed to run skillshub completions");

    assert!(!output.status.success(), "invalid shell should cause a non-zero exit");
}
