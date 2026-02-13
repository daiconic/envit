use async_trait::async_trait;
use azure_core::auth::TokenCredential;
use azure_identity::create_default_credential;
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use std::sync::Arc;

use super::{ProviderError, SecretMeta, SecretProvider};

const API_VERSION: &str = "7.4";
const SCOPE: &str = "https://vault.azure.net/.default";

pub struct AzureKeyVaultProvider {
    vault_url: String,
    credential: Arc<dyn TokenCredential>,
    http: Client,
}

impl AzureKeyVaultProvider {
    pub fn new(vault_url: String) -> Self {
        let credential = create_default_credential().expect("failed to create Azure credential");
        Self {
            vault_url: vault_url.trim_end_matches('/').to_string(),
            credential,
            http: Client::new(),
        }
    }

    async fn access_token(&self) -> Result<String, ProviderError> {
        let token = self
            .credential
            .get_token(&[SCOPE])
            .await
            .map_err(|e| ProviderError::Other(format!("failed to get Azure token: {e}")))?;
        Ok(token.token.secret().to_string())
    }

    async fn get_json<T: for<'de> Deserialize<'de>>(&self, url: &str) -> Result<T, ProviderError> {
        let token = self.access_token().await?;
        let res = self
            .http
            .get(url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| ProviderError::Other(format!("request failed: {e}")))?;

        if res.status().is_success() {
            res.json::<T>()
                .await
                .map_err(|e| ProviderError::Other(format!("invalid response body: {e}")))
        } else {
            Err(ProviderError::Other(format!(
                "key vault request failed ({}) for {}",
                res.status(),
                url
            )))
        }
    }
}

#[derive(Debug, Deserialize)]
struct SecretListResponse {
    value: Vec<SecretListItem>,
    #[serde(rename = "nextLink")]
    next_link: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SecretListItem {
    id: String,
}

#[derive(Debug, Deserialize)]
struct SecretGetResponse {
    value: String,
}

#[async_trait]
impl SecretProvider for AzureKeyVaultProvider {
    async fn list_secrets(&self) -> Result<Vec<SecretMeta>, ProviderError> {
        let mut url = format!("{}/secrets?api-version={API_VERSION}", self.vault_url);
        let mut out = Vec::new();

        loop {
            let page: SecretListResponse = self.get_json(&url).await?;

            for item in page.value {
                if let Some(name) = item
                    .id
                    .split("/secrets/")
                    .nth(1)
                    .and_then(|rest| rest.split('/').next())
                    .filter(|s| !s.is_empty())
                {
                    out.push(SecretMeta {
                        name: name.to_string(),
                    });
                }
            }

            if let Some(next) = page.next_link {
                url = next;
            } else {
                break;
            }
        }

        Ok(out)
    }

    async fn get_secret(&self, name: &str) -> Result<Option<String>, ProviderError> {
        let url = format!("{}/secrets/{}?api-version={API_VERSION}", self.vault_url, name);
        let token = self.access_token().await?;
        let res = self
            .http
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| ProviderError::Other(format!("failed requesting secret {name}: {e}")))?;

        if res.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !res.status().is_success() {
            return Err(ProviderError::Other(format!(
                "failed to get secret {name} ({})",
                res.status()
            )));
        }

        let body: SecretGetResponse = res
            .json()
            .await
            .map_err(|e| ProviderError::Other(format!("invalid response body: {e}")))?;

        Ok(Some(body.value))
    }
}
