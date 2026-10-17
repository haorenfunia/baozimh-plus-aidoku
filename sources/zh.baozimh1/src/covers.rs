use aidoku::alloc::{String, string::ToString as _};

pub fn url(raw: &str) -> String {
	let url = crate::net::absolute_url(raw);
	// The CDN's dynamic thumbnail endpoint can stall; use the cacheable original.
	if is_cover(&url) {
		url.split(['?', '#']).next().unwrap_or(&url).to_string()
	} else {
		url
	}
}

pub fn is_cover(url: &str) -> bool {
	url.starts_with("https://static-tw.baozimh.com/cover/")
		|| url.starts_with("https://static.baozimh.com/cover/")
}

#[cfg(test)]
mod tests {
	use super::*;
	use aidoku_test::aidoku_test;

	#[aidoku_test]
	fn homepage_search_and_old_cached_covers_share_the_original_url() {
		let original = "https://static-tw.baozimh.com/cover/example.jpg";
		assert_eq!(
			url("//static-tw.baozimh.com/cover/example.jpg?w=285&h=375&q=100"),
			original
		);
		assert_eq!(url(original), original);
		assert_eq!(url(&url(original)), original);
		assert_eq!(
			url("/cover/second.jpg"),
			"https://www.twmanga.com/cover/second.jpg"
		);
	}

	#[aidoku_test]
	fn unrelated_images_and_reader_urls_keep_their_parameters() {
		for original in [
			"https://cdn.example/cover/example.jpg?token=123",
			"https://s1.baozimh.com/scomic/example/0.jpg?token=123",
			"https://static-tw.baozimh.com/scomic/example/0.jpg?w=800",
		] {
			assert!(!is_cover(original));
			assert_eq!(url(original), original);
		}
	}
}
