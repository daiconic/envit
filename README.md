# envit

## Usage

### 1. Create `envit.toml`

```toml
version = 1

[output]
env_file = ".env"
create_if_missing = true

[provider]
kind = "azure_key_vault"
vault_url = "https://my-vault.vault.azure.net/"

[map]
DATABASE_URL = "database-url"
```

### 2. Pull secrets and update `.env`

```bash
envit pull
```

### 3. Use a custom config path

```bash
envit pull --config ./envit.toml
```

### 4. Dry run (no file update, values masked)

```bash
envit pull --dry-run
```
