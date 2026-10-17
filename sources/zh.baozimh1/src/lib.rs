#![no_std]

mod banner;
mod banner_signatures;
mod chapter_keys;
mod covers;
mod html;
mod home;
mod json;
mod net;
mod text;

use aidoku::{
	Chapter, DeepLinkHandler, DeepLinkResult, ImageRequestProvider, ImageResponse, Manga,
	Home, HomeLayout, Listing, ListingProvider, MangaPageResult, MigrationHandler, Page, PageContent,
	PageImageProcessor, Result, Source,
	alloc::{String, Vec, string::ToString as _},
	imports::html::Document,
	imports::net::Request,
	prelude::*,
};
use html::{ChapterPage as _, MangaPage as _, PageList as _};
use json::ApiResponse;
use net::{
	APP_ID, APP_USER_AGENT, APP_VERSION, BYPASS_HOSTS, DEVICE_CODE, DEVICE_ID, Url,
};

pub const BASE_URL: &str = "https://www.twmanga.com";

struct Baozimanhua;

impl Source for Baozimanhua {
	fn new() -> Self {
		Self
	}

	fn get_search_manga_list(
		&self,
		query: Option<String>,
		page: i32,
		filters: Vec<aidoku::FilterValue>,
	) -> Result<MangaPageResult> {
		let url = Url::from_query_or_filters(query.as_deref(), page, &filters)?;

		// API request returns JSON
		if let Url::Filter { .. } = &url {
			let request = url.request()?;
			let json_data = request.data()?;
			let response: ApiResponse = serde_json::from_slice(&json_data)?;
			return Ok(response.into());
		}

		// Search and other requests return HTML
		let html = url.request()?.html()?;
		html.manga_page_result()
	}

	fn get_manga_update(
		&self,
		mut manga: Manga,
		needs_details: bool,
		needs_chapters: bool,
	) -> Result<Manga> {
		// Details and chapters come from the same page. Fetching it twice made
		// opening a manga needlessly slow and increased the chance of timeouts.
		if needs_details || needs_chapters {
			let manga_page = Url::manga(manga.key.clone()).request()?.html()?;
			if needs_details {
				manga_page.update_details(&mut manga)?;
			}
			if needs_chapters {
				manga.chapters = Some(manga_page.chapters(&manga.key)?);
			}
		}

		text::manga(&mut manga);
		Ok(manga)
	}

	fn get_page_list(&self, manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let chapter_key = chapter.key.split('|').next().unwrap_or(&chapter.key).to_string();
		let public_url = chapter
			.url
			.clone()
			.unwrap_or_else(|| Url::chapter(manga.key, chapter_key).to_string());

		// App-compatible hosts return the full page list; try them before the
		// public page, which currently caps reader images at 50 for older chapters.
		for host in BYPASS_HOSTS {
			if let Ok(document) = net::app_chapter_request(&public_url, host)?.html()
				&& let Ok(pages) = document.pages()
				&& pages.iter().any(|page| match &page.content {
					PageContent::Url(url, _) => url.contains("baozicdn.com"),
					_ => false,
				})
			{
				return Ok(pages);
			}
		}

		Request::get(public_url)?
			.header("Origin", BASE_URL)
			.header("Referer", BASE_URL)
			.header("Accept-Language", "zh-CN,zh;q=0.9")
			.html()?
			.pages()
	}
}

impl Home for Baozimanhua {
	fn get_home(&self) -> Result<HomeLayout> {
		home::fetch()
	}
}

impl ListingProvider for Baozimanhua {
	fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {
		if listing.id == "all" {
			return self.get_search_manga_list(None, page, Vec::new());
		}
		home::listing(&listing.id, page)
	}
}

impl ImageRequestProvider for Baozimanhua {
	fn get_image_request(
		&self,
		url: String,
		_context: Option<aidoku::PageContext>,
	) -> Result<Request> {
		let url = url
			.replace(".baozicdn.com", ".baozimh.com")
			.replace(".bzcdn.net", ".baozimh.com");
		if covers::is_cover(&url) {
			return Ok(Request::get(covers::url(&url))?.header("Referer", BASE_URL));
		}
		Ok(Request::get(url)?
			.header("Referer", "https://app.baozimh.com/")
			.header("User-Agent", APP_USER_AGENT)
			.header("app-id", APP_ID)
			.header("app-version", APP_VERSION)
			.header("device-code", DEVICE_CODE)
			.header("device-id", DEVICE_ID))
	}
}

impl PageImageProcessor for Baozimanhua {
	fn process_page_image(
		&self,
		response: ImageResponse,
		_context: Option<aidoku::PageContext>,
	) -> Result<aidoku::imports::canvas::ImageRef> {
		Ok(banner::process_image(response))
	}
}

impl DeepLinkHandler for Baozimanhua {
	fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
		let canonical_url = net::absolute_url(&url);
		let url = canonical_url.trim_start_matches(BASE_URL);
		let mut splits = url.split('/').skip(1);
		let deep_link_result = match splits.next() {
			Some("comic") => match splits.next() {
				Some("chapter") => {
					match (splits.next(), splits.next()) {
						(Some(manga_id), Some(chapter_path)) => Some(DeepLinkResult::Chapter {
							manga_key: manga_id.into(),
							key: chapter_path.trim_end_matches(".html").into(),
						}),
						_ => None,
					}
				}
				Some(manga_id) => Some(DeepLinkResult::Manga {
					key: manga_id.into(),
				}),
				None => None,
			},
			_ => None,
		};
		Ok(deep_link_result)
	}
}

impl MigrationHandler for Baozimanhua {
	fn handle_manga_migration(&self, key: String) -> Result<String> {
		Ok(key)
	}

	fn handle_chapter_migration(&self, _manga_key: String, chapter_key: String) -> Result<String> {
		Ok(chapter_keys::stable_key(&chapter_key).to_string())
	}
}

register_source!(
	Baozimanhua,
	ImageRequestProvider,
	PageImageProcessor,
	DeepLinkHandler,
	MigrationHandler,
	Home,
	ListingProvider
);

#[cfg(test)]
mod tests {
	use super::*;
	use aidoku_test::aidoku_test;

	#[aidoku_test]
	fn image_requests_fix_cached_cover_urls_without_changing_reader_paths() {
		let source = Baozimanhua::new();
		let cover = source
			.get_image_request(
				"https://static-tw.baozimh.com/cover/example.jpg?w=285&h=375".into(),
				None,
			)
			.unwrap();
		assert_eq!(
			cover.url().map(String::as_str),
			Some("https://static-tw.baozimh.com/cover/example.jpg")
		);
		let page = source
			.get_image_request(
				"https://s1.baozicdn.com/scomic/example/0.jpg?token=123".into(),
				None,
			)
			.unwrap();
		assert_eq!(
			page.url().map(String::as_str),
			Some("https://s1.baozimh.com/scomic/example/0.jpg?token=123")
		);
	}

	#[aidoku_test]
	fn migration_preserves_manga_and_ordinary_chapter_keys() {
		let source = Baozimanhua::new();
		assert_eq!(
			source.handle_manga_migration("example".into()).unwrap(),
			"example"
		);
		assert_eq!(
			source
				.handle_chapter_migration("example".into(), "0_135".into())
				.unwrap(),
			"0_135"
		);
	}

	#[aidoku_test]
	fn migration_maps_latest_history_to_the_stable_chapter() {
		let source = Baozimanhua::new();
		let migrated = source
			.handle_chapter_migration("example".into(), "0_136|latest".into())
			.unwrap();
		assert_eq!(migrated, "0_136");
		assert_eq!(
			source
				.handle_chapter_migration("example".into(), migrated.clone())
				.unwrap(),
			migrated
		);
	}
}
