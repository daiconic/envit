use std::{fs, path::Path};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn write_file(path: &Path, content: &str) {
    fs::write(path, content).expect("failed to write file");
}

fn write_config(dir: &TempDir, extra_output: &str) {
    let config = format!(
        r#"version = 1

[output]
env_file = ".env"
create_if_missing = true
{}

[provider]
kind = "azure_key_vault"
vault_url = "https://example.vault.azure.net/"
"#,
        extra_output
    );
    write_file(&dir.path().join("envit.toml"), &config);
}

#[test]
fn pull_updates_and_adds_without_deleting_local_only_keys() {
    let dir = TempDir::new().unwrap();
    write_config(&dir, "");

    write_file(
        &dir.path().join(".env"),
        "DATABASE_URL=old\nLOCAL_ONLY=keep\n",
    );
    write_file(
        &dir.path().join("secrets.txt"),
        "database-url=new\nredis=redis://localhost\n",
    );

    Command::new(assert_cmd::cargo::cargo_bin!("envit"))
        .current_dir(dir.path())
        .env("ENVIT_TEST_SECRETS_FILE", dir.path().join("secrets.txt"))
        .arg("pull")
        .assert()
        .success();

    let env_after = fs::read_to_string(dir.path().join(".env")).unwrap();
    assert!(env_after.contains("DATABASE_URL=new"));
    assert!(env_after.contains("REDIS=redis://localhost"));
    assert!(env_after.contains("LOCAL_ONLY=keep"));
}

#[test]
fn pull_preserves_comments_and_order() {
    let dir = TempDir::new().unwrap();
    write_config(&dir, "");

    write_file(
        &dir.path().join(".env"),
        "# header\nDATABASE_URL=old\n\nLOCAL_ONLY=keep",
    );
    write_file(
        &dir.path().join("secrets.txt"),
        "database-url=new\n",
    );

    Command::new(assert_cmd::cargo::cargo_bin!("envit"))
        .current_dir(dir.path())
        .env("ENVIT_TEST_SECRETS_FILE", dir.path().join("secrets.txt"))
        .arg("pull")
        .assert()
        .success();

    let env_after = fs::read_to_string(dir.path().join(".env")).unwrap();
    assert!(env_after.starts_with("# header\nDATABASE_URL=new\n\nLOCAL_ONLY=keep"));
}

#[test]
fn pull_dry_run_shows_changes_but_not_values() {
    let dir = TempDir::new().unwrap();
    write_config(&dir, "");

    write_file(&dir.path().join(".env"), "DATABASE_URL=old\n");
    write_file(
        &dir.path().join("secrets.txt"),
        "database-url=super-secret\nredis=redis://localhost\n",
    );

    Command::new(assert_cmd::cargo::cargo_bin!("envit"))
        .current_dir(dir.path())
        .env("ENVIT_TEST_SECRETS_FILE", dir.path().join("secrets.txt"))
        .arg("pull")
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(predicate::str::contains("UPDATE DATABASE_URL=********"))
        .stdout(predicate::str::contains("ADD REDIS=********"))
        .stdout(predicate::str::contains("super-secret").not())
        .stdout(predicate::str::contains("redis://localhost").not());

    let env_after = fs::read_to_string(dir.path().join(".env")).unwrap();
    assert_eq!(env_after, "DATABASE_URL=old\n");
}

#[test]
fn pull_aborts_without_writing_on_any_fetch_error() {
    let dir = TempDir::new().unwrap();
    write_config(&dir, "");

    let initial = "DATABASE_URL=old\n";
    write_file(&dir.path().join(".env"), initial);
    write_file(
        &dir.path().join("secrets.txt"),
        "database-url=new\n!error:broken-secret\n",
    );

    Command::new(assert_cmd::cargo::cargo_bin!("envit"))
        .current_dir(dir.path())
        .env("ENVIT_TEST_SECRETS_FILE", dir.path().join("secrets.txt"))
        .arg("pull")
        .assert()
        .failure()
        .stderr(predicate::str::contains("failed to fetch secret broken-secret"));

    let after = fs::read_to_string(dir.path().join(".env")).unwrap();
    assert_eq!(after, initial);
}

#[test]
fn pull_errors_when_env_missing_and_create_if_missing_false() {
    let dir = TempDir::new().unwrap();
    let config = r#"version = 1

[output]
env_file = ".env"
create_if_missing = false

[provider]
kind = "azure_key_vault"
vault_url = "https://example.vault.azure.net/"
"#;
    write_file(&dir.path().join("envit.toml"), config);

    write_file(&dir.path().join("secrets.txt"), "database-url=new\n");

    Command::new(assert_cmd::cargo::cargo_bin!("envit"))
        .current_dir(dir.path())
        .env("ENVIT_TEST_SECRETS_FILE", dir.path().join("secrets.txt"))
        .arg("pull")
        .assert()
        .failure()
        .stderr(predicate::str::contains("env file does not exist"));
}
