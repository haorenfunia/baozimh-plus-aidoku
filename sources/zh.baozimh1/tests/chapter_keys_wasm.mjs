import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

// Exercise the actual Rust migration helper without an Aidoku or MSVC runtime.
const directory = dirname(fileURLToPath(import.meta.url));
const temporary = mkdtempSync(join(tmpdir(), "aidoku-chapter-keys-"));
try {
	const output = join(temporary, "chapter_keys.wasm");
	const build = spawnSync("rustc", [
		"--edition=2024",
		"--crate-type=cdylib",
		"--target=wasm32-unknown-unknown",
		"-C", "panic=abort",
		"-O",
		join(directory, "fixtures", "chapter_keys_wasm.rs"),
		"-o", output,
	], { encoding: "utf8" });
	assert.equal(build.status, 0, build.stderr || String(build.error));
	const { instance } = await WebAssembly.instantiate(readFileSync(output), {});
	assert.equal(instance.exports.migration_cases(), 0, "chapter key migration regression");
	console.log("PASS: 6 migration cases and 6 idempotence checks (actual Rust/WASM).");
} finally {
	rmSync(temporary, { recursive: true, force: true });
}
