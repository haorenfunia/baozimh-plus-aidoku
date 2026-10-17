use crate::{BASE_URL, covers, text};
use aidoku::{
	Manga, MangaPageResult,
	alloc::{String, Vec, string::ToString as _, vec},
	prelude::*,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ApiResponse {
	items: Vec<MangaItem>,
	next: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct MangaItem {
	comic_id: String,
	topic_img: String,
	name: String,
	author: String,
	type_names: Vec<String>,
	region_name: Option<String>,
	region: Option<String>,
}

impl From<ApiResponse> for MangaPageResult {
	fn from(response: ApiResponse) -> Self {
		let entries = response
			.items
			.into_iter()
			.map(|item| {
				let key = item.comic_id.clone();
				let cover = Some(covers::url(&format!(
					"https://static-tw.baozimh.com/cover/{}",
					item.topic_img
				)));
				let title = item.name;

				// Deduplicate authors
				let mut artists: Vec<String> = item
					.author
					.split(',')
					.map(|s| s.trim().to_string())
					.filter(|s| !s.is_empty())
					.collect();
				artists.dedup();
				let artist_str = artists.join(", ");

				// Filter out ASCII-only genres and add region
				let mut categories: Vec<String> = item
					.type_names
					.into_iter()
					.filter(|g| !g.is_ascii())
					.collect();

				if let Some(region) = item.region_name.or(item.region) {
					categories.insert(0, region);
				}

				let mut manga = Manga {
					key: key.clone(),
					cover,
					title,
					authors: Some(vec![artist_str]),
					url: Some(format!("{}/comic/{}", BASE_URL, key)),
					tags: Some(categories),
					..Default::default()
				};
				text::manga(&mut manga);
				manga
			})
			.collect();

		let has_next_page = response.next.is_some();

		MangaPageResult {
			entries,
			has_next_page,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use aidoku_test::aidoku_test;

	#[aidoku_test]
	fn category_results_convert_display_text_and_preserve_identifiers() {
		let response: ApiResponse = serde_json::from_str(
			r#"{"items":[{"comic_id":"example-id","topic_img":"example.jpg?w=285","name":"星劍大師","author":"畫師","type_names":["熱血","rexie"],"region_name":"國漫"}],"next":null}"#,
		)
		.unwrap();
		let results: MangaPageResult = response.into();
		let manga = &results.entries[0];
		assert_eq!(manga.key, "example-id");
		assert_eq!(manga.title, "星剑大师");
		assert_eq!(manga.authors.as_ref().unwrap(), &vec!["画师".to_string()]);
		assert_eq!(
			manga.tags.as_ref().unwrap(),
			&vec!["国漫".to_string(), "热血".to_string()]
		);
		assert_eq!(
			manga.cover.as_deref(),
			Some("https://static-tw.baozimh.com/cover/example.jpg")
		);
		assert!(!results.has_next_page);
	}
}
