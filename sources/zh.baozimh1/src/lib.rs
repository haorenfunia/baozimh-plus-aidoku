#![no_std]

mod html;
mod json;
mod net;

use aidoku::{
	Chapter, DeepLinkHandler, DeepLinkResult, ImageRequestProvider, Manga, MangaPageResult, Page,
	Result, Source,
	alloc::{String, Vec},
	imports::html::Document,
	imports::net::Request,
	prelude::*,
};
use html::{ChapterPage as _, MangaPage as _, PageList as _};
use json::ApiResponse;
use net::Url;

pub const BASE_URL: &str = "https://www.baozimh.com";

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
		if needs_details {
			let manga_page = Url::manga(manga.key.clone()).request()?.html()?;
			manga_page.update_details(&mut manga)?;
		}

		if needs_chapters {
			let chapter_page = Url::manga(manga.key.clone()).request()?.html()?;
			manga.chapters = Some(chapter_page.chapters(&manga.key)?);
		}

		Ok(manga)
	}

	fn get_page_list(&self, manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {
		let is_latest = chapter.key.split('|').any(|part| part == LATEST_CHAPTER_MARK);
		let chapter_key = chapter.key.split('|').next().unwrap_or(&chapter.key).to_string();
		let document = if is_latest {
			latest_chapter_document(&manga.key, &chapter_key)?
		} else {
			Url::chapter(manga.key, chapter_key).request()?.html()?
		};
		document.pages()
	}
}

const LATEST_CHAPTER_MARK: &str = "baozimh_latest";
const APP_USER_AGENT: &str = "baozimh_android/1.0.31/gb/adset";
const APP_VERSION: &str = "1.0.31";
const APP_ID: &str = "cn.sts.xiaoyun.ordermeals";
const DEVICE_ID: &str = "BE2A.250530.026.F3";
const DEVICE_CODE: &str = "2c712c6ba4e95a9f4157f94e1794a86c";
const BYPASS_HOSTS: &[&str] = &[
	"appgb-vdkr.baozimh.com",
	"appgb1-vdkr.baozimh.com",
	"appgb2-vdkr.baozimh.com",
	"app1-vdkr.baozimh.com",
	"app2-vdkr.baozimh.com",
];

fn latest_chapter_document(manga_id: &str, chapter_key: &str) -> Result<Document> {
	let path = format!(
		"/baozimhapp/comic/chapter/{}/{}.html",
		manga_id,
		net::chapter_path(chapter_key)
	);

	for host in BYPASS_HOSTS {
		let url = format!("https://{}{}", host, path);
		let request = Request::get(url)?
			.header("Origin", BASE_URL)
			.header("Referer", "https://app.baozimh.com/")
			.header("app-id", APP_ID)
			.header("app-version", APP_VERSION)
			.header("device-code", DEVICE_CODE)
			.header("device-id", DEVICE_ID)
			.header("User-Agent", APP_USER_AGENT);
		if let Ok(document) = request.html()
			&& document
				.select("div.chapter-img img.comic-contain__item[data-src], img.comic-contain__item[data-src]")
				.map(|mut items| items.next().is_some())
				.unwrap_or(false)
		{
			return Ok(document);
		}
	}

	Url::chapter(manga_id.to_string(), chapter_key.to_string())
		.request()?
		.html()
}

impl ImageRequestProvider for Baozimanhua {
	fn get_image_request(
		&self,
		url: String,
		_context: Option<aidoku::PageContext>,
	) -> Result<Request> {
		let url = url.replace(".baozicdn.com", ".baozimh.com");
		Ok(Request::get(url)?.header("Referer", BASE_URL))
	}
}

impl DeepLinkHandler for Baozimanhua {
	fn handle_deep_link(&self, url: String) -> Result<Option<DeepLinkResult>> {
		let url = url.trim_start_matches(BASE_URL);
		let mut splits = url.split('/').skip(1);
		let deep_link_result = match splits.next() {
			Some("comic") => {
		match splits.next() {
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
				}
			}
			_ => None,
		};
		Ok(deep_link_result)
	}
}

register_source!(Baozimanhua, ImageRequestProvider, DeepLinkHandler);
