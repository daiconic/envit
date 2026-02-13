use std::{collections::HashMap, fs, path::Path};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub version: u32,
    #[serde(default)]
    pub output: OutputConfig,
    pub provider: ProviderConfig,
    #[serde(default)]
    pub map: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OutputConfig {
    #[serde(default = "default_env_file")]
    pub env_file: String,
    #[serde(default = "default_create_if_missing")]
    pub create_if_missing: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    pub kind: String,
    pub vault_url: String,
}

fn default_env_file() -> String {
    ".env".to_string()
}

fn default_create_if_missing() -> bool {
    true
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            env_file: default_env_file(),
            create_if_missing: default_create_if_missing(),
        }
    }
}

pub fn load(path: &Path) -> Result<Config> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read config file: {}", path.display()))?;
    let cfg: Config = toml::from_str(&raw)
        .with_context(|| format!("failed to parse TOML config: {}", path.display()))?;
    validate(&cfg)?;
    Ok(cfg)
}

pub fn validate(cfg: &Config) -> Result<()> {
    if cfg.version != 1 {
        bail!("unsupported config version: {} (expected 1)", cfg.version);
    }
    if cfg.provider.kind != "azure_key_vault" {
        bail!(
            "unsupported provider kind: {} (expected azure_key_vault)",
            cfg.provider.kind
        );
    }
    if cfg.provider.vault_url.trim().is_empty() {
        bail!("provider.vault_url must not be empty");
    }
    if cfg.output.env_file.trim().is_empty() {
        bail!("output.env_file must not be empty");
    }
    for (env_key, secret_name) in &cfg.map {
        if env_key.trim().is_empty() || secret_name.trim().is_empty() {
            bail!("[map] entries must not be empty");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_wrong_version() {
        let cfg = Config {
            version: 2,
            output: OutputConfig::default(),
            provider: ProviderConfig {
                kind: "azure_key_vault".to_string(),
                vault_url: "https://example.vault.azure.net".to_string(),
            },
            map: HashMap::new(),
        };

        assert!(validate(&cfg).is_err());
    }
}
