use std::{collections::BTreeMap, env, fmt::Write as _, fs, path::PathBuf};

fn main() {
	let mut dictionary = BTreeMap::new();
	for filename in ["TSCharacters.txt", "TSPhrases.txt"] {
		let path = format!("data/opencc/{filename}");
		println!("cargo:rerun-if-changed={path}");
		for line in fs::read_to_string(path).unwrap().lines() {
			if line.starts_with('#') || line.trim().is_empty() {
				continue;
			}
			let (key, values) = line
				.split_once('\t')
				.expect("Invalid OpenCC dictionary row");
			let value = values
				.split_whitespace()
				.next()
				.expect("Empty OpenCC value");
			assert!(!key.is_empty());
			assert!(
				dictionary
					.insert(key.to_string(), value.to_string())
					.is_none()
			);
		}
	}
	assert!(dictionary.len() > 4_000);
	let max_bytes = dictionary.keys().map(String::len).max().unwrap();
	let mut output = format!(
		"const MAX_KEY_BYTES: usize = {max_bytes};\nconst DICTIONARY: &[(&str, &str)] = &[\n"
	);
	for (key, value) in dictionary {
		writeln!(output, "\t({key:?}, {value:?}),").unwrap();
	}
	output.push_str("];\n");
	let path = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("t2s.rs");
	fs::write(path, output).unwrap();
}
