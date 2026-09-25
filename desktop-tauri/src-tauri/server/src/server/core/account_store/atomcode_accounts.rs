//! AtomCode 账号存储。

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::server::core::providers::atomcode::credentials::Credentials;
use crate::server::core::providers::{kind_id, ProviderKind};
use crate::server::logging;

use super::priority::next_free_priority;
use super::sql;
use super::state::StoredAccount;
use super::store::{AccountStore, AccountStoreError};
use super::store_util::{token_tail_of, truncate_chars};
use super::CredentialWrite;

const PROVIDER: &str = kind_id(ProviderKind::AtmCode);

impl AccountStore {
    pub fn atomcode_account_record(&self, account_id: &str) -> Option<Value> {
        let guard = self.guard();
        if !account_id.is_empty() {
            let record = self.record_by_id(&guard, account_id)?;
            return (record.provider() == PROVIDER).then(|| record.to_value());
        }
        self.records_for_provider(&guard, PROVIDER)
            .into_iter()
            .filter(|record| record.enabled() && record.has_token())
            .min_by_key(|record| record.order_key())
            .map(|record| record.to_value())
    }

    pub fn add_atomcode_account(
        &self,
        credentials: &Credentials,
        name: Option<&str>,
        source: &str,
    ) -> Result<Value, AccountStoreError> {
        let mut credentials = credentials.clone();
        credentials
            .complete_identity()
            .map_err(|error| AccountStoreError::new(error.message, error.status_code))?;
        let guard = self.guard();
        let existing = self
            .records_for_provider(&guard, PROVIDER)
            .into_iter()
            .find(|record| record.uid() == credentials.user_id);
        let id = existing
            .as_ref()
            .map(|record| record.id().to_string())
            .unwrap_or_else(|| {
                format!(
                    "atomcode-{:x}",
                    Sha256::digest(credentials.user_id.as_bytes())
                )
            });
        if existing.is_none() && self.record_by_id(&guard, &id).is_some() {
            return Err(AccountStoreError::new(
                "AtomCode 账号 ID 已被其它账号占用",
                409,
            ));
        }
        let record_name = name
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| {
                existing
                    .as_ref()
                    .map(StoredAccount::name)
                    .filter(|value| !value.is_empty())
            })
            .unwrap_or_else(|| {
                if !credentials.name.is_empty() {
                    credentials.name.clone()
                } else if !credentials.email.is_empty() {
                    credentials.email.clone()
                } else {
                    format!("AtomCode {}", credentials.user_id)
                }
            });
        let mut fields = existing
            .as_ref()
            .map(|record| record.fields().clone())
            .unwrap_or_default();
        if let Value::Object(values) = credentials.to_value() {
            for (key, value) in values {
                if value.is_null() {
                    continue;
                }
                if matches!(&value, Value::String(text) if text.is_empty()) {
                    continue;
                }
                fields.insert(key, value);
            }
        }
        let priority = match existing.as_ref() {
            Some(record) => record.priority(),
            None => {
                let used = self
                    .with_conn(&guard, |conn| sql::priorities_all(conn))
                    .unwrap_or_default();
                next_free_priority(&used)
            }
        };
        fields.insert("id".to_string(), Value::String(id.clone()));
        fields.insert("provider".to_string(), Value::String(PROVIDER.to_string()));
        fields.insert(
            "uid".to_string(),
            Value::String(credentials.user_id.clone()),
        );
        fields.insert(
            "name".to_string(),
            Value::String(truncate_chars(&record_name, 100)),
        );
        fields.insert(
            "nickname".to_string(),
            Value::String(credentials.name.clone()),
        );
        fields.insert(
            "tokenTail".to_string(),
            Value::String(token_tail_of(&credentials.access_token)),
        );
        fields.insert("priority".to_string(), Value::from(priority));
        fields.insert("enabled".to_string(), Value::Bool(true));
        fields.insert("desktop".to_string(), Value::Bool(false));
        fields.insert("source".to_string(), Value::String(source.to_string()));
        fields.insert(
            "addedAt".to_string(),
            Value::from(
                existing
                    .as_ref()
                    .map(StoredAccount::added_at)
                    .unwrap_or_else(logging::now_ms),
            ),
        );
        fields.insert("updatedAt".to_string(), Value::from(logging::now_ms()));
        fields.insert("rateLimits".to_string(), json!({}));
        for key in ["edition", "endpoint", "prefixPath", "platform"] {
            fields.remove(key);
        }
        let record = StoredAccount::from_map(fields);
        self.with_conn(&guard, |conn| sql::put(conn, &record))?;
        logging::log(
            "[Accounts]",
            &format!("✅ AtomCode 账号已保存（优先级 {priority}）"),
        );
        Ok(self.public_account(&record))
    }

    pub fn update_atomcode_credentials_if_current(
        &self,
        expected: &Value,
        credentials: &Credentials,
    ) -> Result<CredentialWrite, AccountStoreError> {
        let id = expected.get("id").and_then(Value::as_str).unwrap_or("");
        let guard = self.guard();
        let Some(mut record) = self
            .record_by_id(&guard, id)
            .filter(|record| record.provider() == PROVIDER)
        else {
            return Ok(CredentialWrite::Stale);
        };
        for key in [
            "accessToken",
            "refreshToken",
            "expiresAt",
            "userId",
            "addedAt",
        ] {
            if record.get(key) != expected.get(key) {
                return Ok(CredentialWrite::Stale);
            }
        }
        if record.uid() != credentials.user_id {
            return Err(AccountStoreError::bad_request(
                "AtomCode 刷新结果与原账号身份不一致",
            ));
        }
        if let Value::Object(values) = credentials.to_value() {
            for (key, value) in values {
                if value.is_null() || matches!(&value, Value::String(text) if text.is_empty()) {
                    continue;
                }
                record.fields_mut().insert(key, value);
            }
        }
        record.set(
            "tokenTail",
            Value::String(token_tail_of(&credentials.access_token)),
        );
        record.set_updated_at(logging::now_ms());
        self.with_conn(&guard, |conn| sql::update_in_place(conn, &record))?;
        Ok(CredentialWrite::Written)
    }

    pub fn to_atomcode_public_account(&self, record: &StoredAccount) -> Value {
        let mut public = Map::new();
        for key in [
            "id",
            "provider",
            "name",
            "nickname",
            "uid",
            "email",
            "source",
            "tokenTail",
            "expiresAt",
        ] {
            public.insert(
                key.to_string(),
                record.get(key).cloned().unwrap_or(Value::Null),
            );
        }
        public.insert(
            "userId".to_string(),
            record.get("uid").cloned().unwrap_or(Value::Null),
        );
        public.insert(
            "hasRefreshToken".to_string(),
            Value::Bool(!record.refresh_token().is_empty()),
        );
        public.insert("priority".to_string(), Value::from(record.priority()));
        public.insert("enabled".to_string(), Value::Bool(record.enabled()));
        public.insert("addedAt".to_string(), Value::from(record.added_at()));
        public.insert("updatedAt".to_string(), Value::from(record.updated_at()));
        public.insert(
            "proxy".to_string(),
            crate::server::core::proxies::describe_account_proxy(Some(&record.proxy())),
        );
        public.insert(
            "rateLimits".to_string(),
            record
                .get("rateLimits")
                .cloned()
                .unwrap_or_else(|| json!({})),
        );
        public.insert("desktop".to_string(), Value::Bool(false));
        public.insert("available".to_string(), Value::Bool(record.has_token()));
        Value::Object(public)
    }
}
