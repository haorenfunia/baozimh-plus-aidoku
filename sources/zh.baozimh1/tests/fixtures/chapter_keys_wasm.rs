#![no_std]

#[path = "../../src/chapter_keys.rs"]
mod chapter_keys;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
	core::arch::wasm32::unreachable()
}

#[unsafe(no_mangle)]
pub extern "C" fn migration_cases() -> u32 {
	let cases = [
		("0_136|latest", "0_136"),
		("0_135", "0_135"),
		("1_137|latest", "1_137"),
		("", ""),
		("0_136|other", "0_136|other"),
		("latest", "latest"),
	];
	let mut failures = 0;
	for (index, (input, expected)) in cases.iter().enumerate() {
		if chapter_keys::stable_key(input) != *expected {
			failures |= 1 << index;
		}
		if chapter_keys::stable_key(chapter_keys::stable_key(input)) != *expected {
			failures |= 1 << (index + 8);
		}
	}
	failures
}
