pub mod azure_key_vault;

use std::{collections::HashMap, env, fs, path::Path};

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use thiserror::Error;

use crate::config::ProviderConfig;

#[derive(Debug, Clone)]
pub struct SecretMeta {
    pub name: String,
}

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("provider transport/auth error: {0}")]
    Other(String),
}

/// Provider contract:
/// - get_secret returns Ok(None) for NotFound
/// - auth/network and other failures return Err
#[async_trait]
pub trait SecretProvider: Send + Sync {
    async fn list_secrets(&self) -> Result<Vec<SecretMeta>, ProviderError>;
    async fn get_secret(&self, name: &str) -> Result<Option<String>, ProviderError>;
}

pub fn build_provider(cfg: &ProviderConfig) -> Result<Box<dyn SecretProvider>> {
    if let Ok(path) = env::var("ENVIT_TEST_SECRETS_FILE") {
        return Ok(Box::new(FixtureProvider::from_file(Path::new(&path))?));
    }

    match cfg.kind.as_str() {
        "azure_key_vault" => Ok(Box::new(azure_key_vault::AzureKeyVaultProvider::new(
            cfg.vault_url.clone(),
        ))),
        other => Err(anyhow!("unsupported provider kind: {other}")),
    }
}

#[derive(Debug, Default)]
struct FixtureProvider {
    listed: Vec<String>,
    values: HashMap<String, String>,
    error_on_get: Vec<String>,
    missing_on_get: Vec<String>,
}

impl FixtureProvider {
    fn from_file(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read fixture secrets file: {}", path.display()))?;

        let mut provider = Self::default();
        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Some(name) = trimmed.strip_prefix("!error:") {
                provider.error_on_get.push(name.trim().to_string());
                provider.listed.push(name.trim().to_string());
                continue;
            }
            if let Some(name) = trimmed.strip_prefix("!missing:") {
                provider.missing_on_get.push(name.trim().to_string());
                provider.listed.push(name.trim().to_string());
                continue;
            }

            let Some(idx) = trimmed.find('=') else {
                return Err(anyhow!("invalid fixture entry: {trimmed}"));
            };

            let name = trimmed[..idx].trim();
            let value = trimmed[idx + 1..].to_string();
            if name.is_empty() {
                return Err(anyhow!("invalid fixture entry (empty name): {trimmed}"));
            }
            provider.listed.push(name.to_string());
            provider.values.insert(name.to_string(), value);
        }

        provider.listed.sort();
        provider.listed.dedup();
        Ok(provider)
    }
}

#[async_trait]
impl SecretProvider for FixtureProvider {
    async fn list_secrets(&self) -> Result<Vec<SecretMeta>, ProviderError> {
        Ok(self
            .listed
            .iter()
            .map(|name| SecretMeta { name: name.clone() })
            .collect())
    }

    async fn get_secret(&self, name: &str) -> Result<Option<String>, ProviderError> {
        if self.error_on_get.iter().any(|it| it == name) {
            return Err(ProviderError::Other(format!(
                "fixture induced get error for secret: {name}"
            )));
        }
        if self.missing_on_get.iter().any(|it| it == name) {
            return Ok(None);
        }
        Ok(self.values.get(name).cloned())
    }
}
