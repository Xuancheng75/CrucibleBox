// 随机 token 生成（1.9.2-a）
// - random_token_hex(64)：renderer session token（对等 randomBytes(32).hex）
// - random_token_alnum(n)：sidecar argv token（对等 PluginSandbox 的随机 token）

const ALNUM: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-";

/// 生成 n 位 [A-Za-z0-9_-] 随机串（>= 32 位满足信封 token 正则）
pub fn random_token_alnum(n: usize) -> Result<String, String> {
    if n < 1 {
        return Err("token length must be >= 1".into());
    }
    let mut buf = vec![0u8; n];
    getrandom::getrandom(&mut buf).map_err(|e| format!("rng failure: {e}"))?;
    Ok(buf.iter().map(|b| ALNUM[(*b as usize) % ALNUM.len()] as char).collect())
}

/// 生成 64 位小写 hex（对等 randomBytes(32).toString('hex')）
#[allow(dead_code)] // 供 renderer session token 未来复用（plugin_session.rs 现有内联实现）
pub fn random_token_hex() -> Result<String, String> {
    let mut buf = [0u8; 32];
    getrandom::getrandom(&mut buf).map_err(|e| format!("rng failure: {e}"))?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alnum_matches_envelope_token_regex() {
        let t = random_token_alnum(32).unwrap();
        assert_eq!(t.len(), 32);
        assert!(t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'));
    }

    #[test]
    fn hex_is_64_lowercase() {
        let t = random_token_hex().unwrap();
        assert_eq!(t.len(), 64);
        assert!(t.chars().all(|c| c.is_ascii_hexdigit() && c.is_lowercase() || c.is_ascii_digit()));
    }

    #[test]
    fn tokens_are_unique() {
        let a = random_token_alnum(32).unwrap();
        let b = random_token_alnum(32).unwrap();
        assert_ne!(a, b);
    }
}
