pub fn stable_key(key: &str) -> &str {
	key.strip_suffix("|latest").unwrap_or(key)
}
