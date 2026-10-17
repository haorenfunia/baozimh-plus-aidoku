use aidoku::alloc::String;

include!(concat!(env!("OUT_DIR"), "/t2s.rs"));

/// OpenCC's longest phrase match takes priority over individual character rules.
pub fn simplified(mut text: &str) -> String {
	let mut output = String::with_capacity(text.len());
	while !text.is_empty() {
		let mut end = text.len().min(MAX_KEY_BYTES);
		while !text.is_char_boundary(end) {
			end -= 1;
		}
		let mut matched = false;
		while end > 0 {
			if let Ok(index) = DICTIONARY.binary_search_by_key(&&text[..end], |(key, _)| *key) {
				output.push_str(DICTIONARY[index].1);
				text = &text[end..];
				matched = true;
				break;
			}
			end = text[..end].char_indices().next_back().unwrap().0;
		}
		if !matched {
			let next = text.chars().next().unwrap();
			output.push(next);
			text = &text[next.len_utf8()..];
		}
	}
	output
}

pub fn manga(manga: &mut aidoku::Manga) {
	manga.title = simplified(&manga.title);
	if let Some(description) = &mut manga.description {
		*description = simplified(description);
	}
	for values in [&mut manga.authors, &mut manga.artists, &mut manga.tags] {
		if let Some(values) = values {
			for value in values {
				*value = simplified(value);
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use aidoku::alloc::{string::ToString as _, vec};
	use aidoku_test::aidoku_test;

	#[aidoku_test]
	fn converts_display_text_with_phrase_exceptions() {
		for (input, expected) in [
			("", ""),
			(
				"熱門漫畫 推薦國漫 推薦韓漫 推薦日漫 熱血漫畫",
				"热门漫画 推荐国漫 推荐韩漫 推荐日漫 热血漫画",
			),
			(
				"測試漫畫：第136話，星劍大師與龍。",
				"测试漫画：第136话，星剑大师与龙。",
			),
			(
				"乾坤大挪移 乾隆年間 乾乾淨淨",
				"乾坤大挪移 乾隆年间 干干净净",
			),
			("一目瞭然 憑藉 記憶與憂鬱", "一目了然 凭借 记忆与忧郁"),
			("ABC 123 日本語 🎨", "ABC 123 日本语 🎨"),
		] {
			assert_eq!(simplified(input), expected);
			assert_eq!(simplified(expected), expected);
		}
	}

	#[aidoku_test]
	fn conversion_only_changes_manga_display_fields() {
		let mut item = aidoku::Manga {
			key: "漫畫-id".into(),
			url: Some("https://www.twmanga.com/comic/漫畫-id".into()),
			cover: Some("https://static-tw.baozimh.com/cover/漫畫-id.jpg".into()),
			title: "測試漫畫".into(),
			description: Some("簡介與記憶".into()),
			authors: Some(vec!["作畫老師".into()]),
			artists: Some(vec!["畫師".into()]),
			tags: Some(vec!["熱血".into()]),
			..Default::default()
		};
		manga(&mut item);
		assert_eq!(item.title, "测试漫画");
		assert_eq!(item.description.as_deref(), Some("简介与记忆"));
		assert_eq!(item.authors.unwrap(), vec!["作画老师".to_string()]);
		assert_eq!(item.artists.unwrap(), vec!["画师".to_string()]);
		assert_eq!(item.tags.unwrap(), vec!["热血".to_string()]);
		assert_eq!(item.key, "漫畫-id");
		assert_eq!(
			item.url.as_deref(),
			Some("https://www.twmanga.com/comic/漫畫-id")
		);
		assert_eq!(
			item.cover.as_deref(),
			Some("https://static-tw.baozimh.com/cover/漫畫-id.jpg")
		);
	}

	#[aidoku_test]
	fn bundled_dictionary_is_complete_sorted_and_executable() {
		assert!(DICTIONARY.len() > 4_000);
		assert!(DICTIONARY.windows(2).all(|pair| pair[0].0 < pair[1].0));
		for &(key, expected) in DICTIONARY {
			assert_eq!(simplified(key), expected, "Dictionary entry: {}", key);
		}
	}
}
