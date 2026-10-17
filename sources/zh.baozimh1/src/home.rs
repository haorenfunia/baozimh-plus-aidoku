use crate::{BASE_URL, net::absolute_url};
use aidoku::{
	HomeComponent, HomeComponentValue, HomeLayout, Link, LinkValue, Listing, Manga,
	MangaPageResult, Result,
	alloc::{String, Vec, string::ToString as _, vec},
	imports::{
		html::{Document, Element},
		net::Request,
	},
	prelude::*,
};

fn text(value: String) -> String {
	value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn cover_url(value: String) -> String {
	if value.starts_with("//") {
		format!("https:{}", value)
	} else if value.starts_with('/') {
		format!("{}{}", BASE_URL, value)
	} else {
		value
	}
}

pub fn cards(container: &Element) -> Vec<Link> {
	container
		.select(".comics-card")
		.into_iter()
		.flatten()
		.filter_map(|card| {
			let poster = card.select_first("a.comics-card__poster[href]")?;
			let url = absolute_url(&poster.attr("href")?);
			let key = url
				.strip_prefix(&format!("{}/comic/", BASE_URL))?
				.split(['?', '#'])
				.next()?
				.trim_matches('/');
			if key.is_empty() || key.contains('/') {
				return None;
			}
			let title = text(
				card.select_first("h3")
					.and_then(|el| el.text())
					.or_else(|| poster.attr("title"))
					.or_else(|| poster.attr("aria-label"))?,
			);
			if title.is_empty() {
				return None;
			}
			let cover = poster
				.select_first(
					"amp-img[noloading], img[data-src], img[src], amp-img[src]:not([placeholder]):not([fallback])",
				)
				.and_then(|el| {
					el.attr("data-src")
						.filter(|s| !s.is_empty())
						.or_else(|| el.attr("src"))
				})
				.filter(|s| !s.is_empty())
				.map(cover_url);
			let subtitle = card
				.select_first("small")
				.and_then(|el| el.text())
				.map(text)
				.filter(|s| !s.is_empty());
			let authors = subtitle
				.as_ref()
				.filter(|s| !s.starts_with("更新至"))
				.map(|s| {
					s.split([',', '，'])
						.map(str::trim)
						.filter(|s| !s.is_empty())
						.map(str::to_string)
						.collect()
				});
			let tags = card
				.select(".tabs .tab")
				.into_iter()
				.flatten()
				.filter_map(|el| el.text())
				.map(text)
				.filter(|s| !s.is_empty())
				.collect();
			let manga = Manga {
				key: key.to_string(),
				title: title.clone(),
				cover: cover.clone(),
				url: Some(url),
				authors,
				tags: Some(tags),
				..Default::default()
			};
			Some(Link {
				title,
				subtitle,
				image_url: cover,
				value: Some(LinkValue::Manga(manga)),
			})
		})
		.collect()
}

pub fn parse(document: &Document) -> Result<HomeLayout> {
	let mut components = Vec::new();
	for section in document
		.select(".index-recommend-items")
		.into_iter()
		.flatten()
	{
		let Some(title) = section
			.select_first(".catalog-title")
			.and_then(|el| el.text())
			.map(text)
			.filter(|s| !s.is_empty())
		else {
			continue;
		};
		let entries = cards(&section);
		if entries.is_empty() {
			continue;
		}
		let listing = Some(Listing {
			id: format!("home:{}", title),
			name: title.clone(),
			..Default::default()
		});
		let value = if title == "最近更新" {
			HomeComponentValue::MangaList {
				ranking: false,
				page_size: Some(6),
				entries,
				listing,
			}
		} else {
			HomeComponentValue::Scroller { entries, listing }
		};
		components.push(HomeComponent {
			title: Some(title),
			value,
			..Default::default()
		});
	}
	if components.is_empty() {
		bail!("No homepage sections found");
	}
	Ok(HomeLayout { components })
}

pub fn fetch() -> Result<HomeLayout> {
	let document = Request::get(format!("{}/", BASE_URL))?
		.header("Referer", BASE_URL)
		.header("Accept-Language", "zh-CN,zh;q=0.9")
		.html()?;
	parse(&document)
}

pub fn listing(id: &str, page: i32) -> Result<MangaPageResult> {
	// The site's curated homepage and new-arrivals page are finite, not paginated.
	if page > 1 {
		return Ok(MangaPageResult {
			entries: vec![],
			has_next_page: false,
		});
	}
	let entries = if id == "new" {
		let document = Request::get(format!("{}/list/new", BASE_URL))?
			.header("Referer", BASE_URL)
			.html()?;
		cards(&Element::from(document))
	} else {
		let title = id
			.strip_prefix("home:")
			.ok_or_else(|| error!("Unknown listing"))?;
		let component = fetch()?
			.components
			.into_iter()
			.find(|component| component.title.as_deref() == Some(title))
			.ok_or_else(|| error!("Homepage section not found: {}", title))?;
		match component.value {
			HomeComponentValue::Scroller { entries, .. }
			| HomeComponentValue::MangaList { entries, .. } => entries,
			_ => vec![],
		}
	};
	Ok(MangaPageResult {
		entries: entries
			.into_iter()
			.filter_map(|link| match link.value {
				Some(LinkValue::Manga(manga)) => Some(manga),
				_ => None,
			})
			.collect(),
		has_next_page: false,
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use aidoku::{Source, imports::html::Html};
	use aidoku_test::aidoku_test;

	#[aidoku_test]
	fn homepage_preserves_sections_order_and_metadata() {
		let document = Html::parse(include_str!("../tests/fixtures/homepage.html")).unwrap();
		let layout = parse(&document).unwrap();
		assert_eq!(layout.components.len(), 3);
		assert_eq!(layout.components[0].title.as_deref(), Some("熱門漫畫"));
		assert_eq!(layout.components[1].title.as_deref(), Some("推薦日漫"));
		assert_eq!(layout.components[2].title.as_deref(), Some("最近更新"));
		let HomeComponentValue::Scroller { entries, listing } = &layout.components[0].value else {
			panic!("Expected horizontal scroller");
		};
		assert_eq!(entries.len(), 2);
		assert_eq!(listing.as_ref().unwrap().id, "home:熱門漫畫");
		assert_eq!(entries[0].subtitle.as_deref(), Some("更新至 第136話"));
		let Some(LinkValue::Manga(manga)) = &entries[0].value else {
			panic!()
		};
		assert_eq!(manga.key, "example");
		assert_eq!(manga.title, "測試 & 漫畫");
		assert_eq!(
			manga.url.as_deref(),
			Some("https://www.twmanga.com/comic/example")
		);
		assert_eq!(
			manga.cover.as_deref(),
			Some("https://static-tw.baozimh.com/cover/example.jpg?w=285&h=375")
		);
		assert!(manga.authors.is_none());
		assert_eq!(manga.tags.as_ref().unwrap(), &vec!["熱血".to_string()]);
		let HomeComponentValue::Scroller { entries, .. } = &layout.components[1].value else {
			panic!()
		};
		let Some(LinkValue::Manga(manga)) = &entries[0].value else {
			panic!()
		};
		assert_eq!(
			manga.authors.as_ref().unwrap(),
			&vec!["作者一".to_string(), "作者二".to_string()]
		);
		assert_eq!(
			manga.cover.as_deref(),
			Some("https://www.twmanga.com/cover/second.jpg")
		);
		let HomeComponentValue::MangaList {
			entries, page_size, ..
		} = &layout.components[2].value
		else {
			panic!()
		};
		assert_eq!(*page_size, Some(6));
		assert_eq!(entries[0].subtitle.as_deref(), Some("更新至 第137話"));
		// The same manga can legitimately appear in more than one website section.
		let Some(LinkValue::Manga(manga)) = &entries[0].value else {
			panic!()
		};
		assert_eq!(manga.key, "example");
	}

	#[aidoku_test]
	fn empty_homepage_is_an_error_and_finite_lists_stop() {
		assert!(parse(&Html::parse("<html>Maintenance</html>").unwrap()).is_err());
		let source = crate::Baozimanhua::new();
		let result = aidoku::ListingProvider::get_manga_list(
			&source,
			Listing {
				id: "home:熱門漫畫".into(),
				..Default::default()
			},
			2,
		)
		.unwrap();
		assert!(result.entries.is_empty());
		assert!(!result.has_next_page);
	}

	#[aidoku_test]
	#[ignore]
	fn live_home_matches_every_website_card() {
		let document = Request::get(format!("{}/", BASE_URL))
			.unwrap()
			.html()
			.unwrap();
		let layout = parse(&document).unwrap();
		let sections: Vec<_> = document.select(".index-recommend-items").unwrap().collect();
		assert_eq!(layout.components.len(), sections.len());
		let mut total = 0;
		for (component, section) in layout.components.iter().zip(&sections) {
			let title = text(
				section
					.select_first(".catalog-title")
					.unwrap()
					.text()
					.unwrap(),
			);
			assert_eq!(component.title.as_deref(), Some(title.as_str()));
			let entries = match &component.value {
				HomeComponentValue::Scroller { entries, .. }
				| HomeComponentValue::MangaList { entries, .. } => entries,
				_ => panic!("Unexpected component"),
			};
			let posters: Vec<_> = section
				.select("a.comics-card__poster[href]")
				.unwrap()
				.collect();
			assert_eq!(entries.len(), posters.len(), "Missing items in {}", title);
			for (entry, poster) in entries.iter().zip(posters) {
				let Some(LinkValue::Manga(manga)) = &entry.value else {
					panic!()
				};
				assert_eq!(
					manga.url.as_deref(),
					Some(absolute_url(&poster.attr("href").unwrap()).as_str())
				);
				assert!(
					manga
						.cover
						.as_ref()
						.is_some_and(|url| url.starts_with("https://"))
				);
			}
			total += entries.len();
			println!("{}: {} entries", title, entries.len());
		}
		println!("Homepage total: {} entries", total);
	}
}
