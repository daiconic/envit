# envit — 設計方針まとめ

## 目的
- クラウドのシークレットストア（まずは Azure Key Vault）から secret を取得し、ローカルの `.env` ファイルを生成・更新する CLI
- チーム内での `.env` の DM 共有をなくし、取得手順を標準化する
- secret の履歴管理・バージョン管理は行わない
- 正は常にクラウド側

---

## 対象範囲
- Provider: Azure Key Vault のみ（初期）
- 拡張性: Provider は trait / interface で抽象化し、将来追加可能
- 同期単位: 1 secret = 1 env var
- 同期方式: merge
- クラウドに存在する secret → `.env` を更新 or 追加
- クラウドに存在しない secret → `.env` の既存値を保持
- `.env` にのみ存在するキーは削除しない

---

## 自動マッピング仕様（固定）

### ルール（by_name_rule）
- secret 名 → env key の変換規則は固定
1. `-` を `_` に変換
2. 大文字化

### 例
| Key Vault secret | env key |
| --- | --- |
| `database-url` | `DATABASE_URL` |
| `azure-client-id` | `AZURE_CLIENT_ID` |
| `redis` | `REDIS` |

- `prefix` / `tag` / 環境名などは扱わない
- secret 名は Key Vault 側で環境ごとに分ける（vault分離、または別リソース）
- 例外的なキー名調整が必要な場合のみ、手動マッピングを使う

---

## 設定ファイル
- ファイル名: `envit.toml`
- 値（secret）は書かない
- 書くのは「どこから取るか」「どこに出すか」「必要なら上書きマッピング」

### 例
```toml
version = 1

[output]
env_file = ".env"
create_if_missing = true

[provider]
kind = "azure_key_vault"
vault_url = "https://my-vault.vault.azure.net/"

# 自動変換ルールで合わない場合のみ指定（任意）
[map]
DATABASE_URL = "database-url"
```

---

## CLI
- `envit pull` のみ（MVP）
- クラウドから secret を取得
- 変換ルールに従って env key を生成
- `.env` を merge 更新
- オプション（最低限）
- `--config`（デフォルト `envit.toml`）
- `--dry-run`（変更内容のみ表示、値はマスク）

---

## Provider abstraction

### 必須メソッド
- `get_secret(name) -> Result<Option<String>>`
- NotFound → `Ok(None)`
- 認証/通信エラー → `Err`

### 追加メソッド（自動マッピング用）
- `list_secrets() -> Result<Vec<SecretMeta>>`
- `SecretMeta` は最低限 `name`
- ページングは provider 実装側で吸収

※ 自動マッピングが前提なので、Azure Key Vault 実装では list は必須。

---

## セーフティポリシー
- ログ・標準出力・diff に値は出さない
- 取得中にエラーが発生した場合は `.env` を更新しない
- `.env` のコメント・順序は極力保持（詳細実装は後決め）

---

## この設計の性質
- 思想は「同期」ではなく「取得」
- `.env` は生成物であり、管理対象ではない
- envit は「Git for env」ではなく「secret-backed `.env` materializer」
