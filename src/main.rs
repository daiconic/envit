mod config;
mod envfile;
mod provider;

use std::{collections::HashMap, path::{Path, PathBuf}};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use config::Config;
use envfile::ChangeKind;
use provider::build_provider;

#[derive(Debug, Parser)]
#[command(name = "envit")]
#[command(about = "Secret-backed .env materializer")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Pull {
        #[arg(long, default_value = "envit.toml")]
        config: PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Pull { config, dry_run } => run_pull(&config, dry_run).await,
    }
}

async fn run_pull(config_path: &Path, dry_run: bool) -> Result<()> {
    let cfg = config::load(config_path)?;
    let env_path = resolve_env_path(config_path, &cfg);

    let provider = build_provider(&cfg.provider)?;
    let listed = provider
        .list_secrets()
        .await
        .map_err(|e| anyhow::anyhow!("failed to list secrets: {e}"))?;

    let reverse_map = build_reverse_map(&cfg.map)?;
    let mut target_secret_to_env = Vec::with_capacity(listed.len());
    for meta in listed {
        let env_key = reverse_map
            .get(&meta.name)
            .cloned()
            .unwrap_or_else(|| to_env_key(&meta.name));
        target_secret_to_env.push((meta.name, env_key));
    }

    validate_no_duplicate_env_keys(&target_secret_to_env)?;

    let mut updates = HashMap::new();
    for (secret_name, env_key) in target_secret_to_env {
        let value = provider
            .get_secret(&secret_name)
            .await
            .map_err(|e| anyhow::anyhow!("failed to fetch secret {secret_name}: {e}"))?;

        if let Some(value) = value {
            updates.insert(env_key, value);
        }
    }

    let existing = envfile::load_for_merge(&env_path, cfg.output.create_if_missing)?;
    let (merged_content, changes) = envfile::merge(existing, &updates);

    if dry_run {
        print_dry_run(&changes);
        return Ok(());
    }

    if changes.is_empty() && env_path.exists() {
        println!("No changes.");
        return Ok(());
    }

    envfile::write_atomic(&env_path, &merged_content)
        .with_context(|| format!("failed to write {}", env_path.display()))?;

    println!("Updated {} keys in {}", changes.len(), env_path.display());
    Ok(())
}

fn resolve_env_path(config_path: &Path, cfg: &Config) -> PathBuf {
    let env_path = PathBuf::from(&cfg.output.env_file);
    if env_path.is_absolute() {
        return env_path;
    }

    if let Some(parent) = config_path.parent() {
        return parent.join(env_path);
    }

    env_path
}

fn build_reverse_map(map: &HashMap<String, String>) -> Result<HashMap<String, String>> {
    let mut reverse = HashMap::with_capacity(map.len());
    for (env_key, secret_name) in map {
        if let Some(existing) = reverse.insert(secret_name.clone(), env_key.clone()) {
            bail!(
                "duplicate manual mapping for secret {secret_name}: {existing} and {env_key}"
            );
        }
    }
    Ok(reverse)
}

fn validate_no_duplicate_env_keys(pairs: &[(String, String)]) -> Result<()> {
    let mut seen: HashMap<&str, &str> = HashMap::new();
    for (secret, key) in pairs {
        if let Some(existing_secret) = seen.insert(key, secret) {
            bail!(
                "duplicate env key mapping detected: {key} mapped from both {existing_secret} and {secret}"
            );
        }
    }
    Ok(())
}

fn to_env_key(secret_name: &str) -> String {
    secret_name.replace('-', "_").to_ascii_uppercase()
}

fn print_dry_run(changes: &[envfile::Change]) {
    if changes.is_empty() {
        println!("No changes.");
        return;
    }

    for change in changes {
        let label = match change.kind {
            ChangeKind::Add => "ADD",
            ChangeKind::Update => "UPDATE",
        };
        println!("{label} {}=********", change.key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_name_to_env_key_rule() {
        assert_eq!(to_env_key("database-url"), "DATABASE_URL");
        assert_eq!(to_env_key("azure-client-id"), "AZURE_CLIENT_ID");
        assert_eq!(to_env_key("redis"), "REDIS");
    }
}
