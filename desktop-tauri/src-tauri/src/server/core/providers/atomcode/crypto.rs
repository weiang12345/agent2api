//! AtomCode 云直连请求签名（atomcode-signing-v1）。

use sha2::{Digest, Sha256};

use crate::server::errors::GatewayError;

const MASTER_KEY_HEX: &str = "e97250f05303162c8ecd68c688b2f55c1d81e508d243d88466472e7f54637123";
const ALGORITHM: &str = "atomcode-signing-v1";

pub struct SignatureInput<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub body: &'a [u8],
    pub oauth_token: &'a str,
    pub user_id: &'a str,
    pub client_version: &'a str,
    pub timestamp: i64,
    pub nonce: &'a [u8],
}

pub fn build_auth_headers(input: &SignatureInput<'_>) -> Result<Vec<(String, String)>, GatewayError> {
    if input.oauth_token.is_empty() || input.user_id.is_empty() {
        return Err(GatewayError::with_status(401, "AtomCode 账号缺少用户或凭证，无法签名"));
    }
    if input.nonce.len() != 16 {
        return Err(GatewayError::with_status(500, "AtomCode 签名 nonce 长度无效"));
    }
    let master_key = decode_master_key()?;
    let token_hash = Sha256::digest(input.oauth_token.as_bytes());
    let version_hash = Sha256::digest(input.client_version.as_bytes());
    let time_bucket = (input.timestamp / 3600) as u64;
    let mut salt = Vec::with_capacity(input.user_id.len() + 1 + 8 + 64);
    salt.extend_from_slice(input.user_id.as_bytes());
    salt.push(1);
    salt.extend_from_slice(&time_bucket.to_le_bytes());
    salt.extend_from_slice(&token_hash);
    salt.extend_from_slice(&version_hash);

    let pseudorandom_key = hmac_sha256(&salt, &master_key);
    let mut info = Vec::with_capacity(ALGORITHM.len() + 1);
    info.extend_from_slice(ALGORITHM.as_bytes());
    info.push(1);
    let signing_key = hmac_sha256(&pseudorandom_key, &info);

    let body_hash = Sha256::digest(input.body);
    let nonce_hex = hex_encode(input.nonce);
    let canonical = format!(
        "v1\n{}\n{}\n{}\n{}\n{}",
        input.method.to_uppercase(),
        input.path,
        input.timestamp,
        nonce_hex,
        hex_encode(&body_hash)
    );
    let signature = hmac_sha256(&signing_key, canonical.as_bytes());
    Ok(vec![
        ("X-AtomCode-Sig".to_string(), format!("v1:{}", hex_encode(&signature))),
        ("X-AtomCode-Ts".to_string(), input.timestamp.to_string()),
        ("X-AtomCode-Nonce".to_string(), nonce_hex),
        ("X-AtomCode-Alg".to_string(), "1".to_string()),
        ("X-AtomCode-Ver".to_string(), input.client_version.to_string()),
    ])
}

pub fn random_nonce() -> Result<Vec<u8>, GatewayError> {
    let mut nonce = vec![0u8; 16];
    getrandom::getrandom(&mut nonce)
        .map_err(|_| GatewayError::with_status(500, "无法生成 AtomCode 签名随机数"))?;
    Ok(nonce)
}

fn decode_master_key() -> Result<Vec<u8>, GatewayError> {
    let key = hex_decode(MASTER_KEY_HEX)
        .ok_or_else(|| GatewayError::with_status(500, "AtomCode 签名主密钥无效"))?;
    if key.len() != 32 {
        return Err(GatewayError::with_status(500, "AtomCode 签名主密钥长度无效"));
    }
    Ok(key)
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    const BLOCK_SIZE: usize = 64;
    let key_material = if key.len() > BLOCK_SIZE {
        Sha256::digest(key).to_vec()
    } else {
        key.to_vec()
    };
    let mut key_block = vec![0u8; BLOCK_SIZE];
    key_block[..key_material.len()].copy_from_slice(&key_material);
    let mut inner = Sha256::new();
    inner.update(key_block.iter().map(|byte| byte ^ 0x36).collect::<Vec<_>>());
    inner.update(data);
    let inner_hash = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(key_block.iter().map(|byte| byte ^ 0x5c).collect::<Vec<_>>());
    outer.update(inner_hash);
    outer.finalize().to_vec()
}

fn hex_encode(data: &[u8]) -> String {
    data.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hex_decode(value: &str) -> Option<Vec<u8>> {
    if value.len() % 2 != 0 {
        return None;
    }
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_matches_independent_fixture() {
        let nonce: Vec<u8> = (0u8..16).collect();
        let input = SignatureInput {
            method: "POST",
            path: "/v1/chat/completions",
            body: br#"{"model":"deepseek-v4-flash"}"#.as_slice(),
            oauth_token: "token-fixture",
            user_id: "user-fixture",
            client_version: "5.0.2",
            timestamp: 1767225600,
            nonce: &nonce,
        };

        let headers = build_auth_headers(&input).expect("sign AtomCode request");
        assert_eq!(
            headers
                .iter()
                .find(|(name, _)| name == "X-AtomCode-Sig")
                .map(|(_, value)| value.as_str()),
            Some("v1:c5bafa716c0fdc31a6e2738412d0b21a9d9e9d1ca09cc44eb17949a1b4fa4bdd")
        );
        assert_eq!(
            headers
                .iter()
                .find(|(name, _)| name == "X-AtomCode-Nonce")
                .map(|(_, value)| value.as_str()),
            Some("000102030405060708090a0b0c0d0e0f")
        );
        assert_eq!(
            headers
                .iter()
                .find(|(name, _)| name == "X-AtomCode-Ts")
                .map(|(_, value)| value.as_str()),
            Some("1767225600")
        );
        assert_eq!(
            headers
                .iter()
                .find(|(name, _)| name == "X-AtomCode-Ver")
                .map(|(_, value)| value.as_str()),
            Some("5.0.2")
        );
    }
}
