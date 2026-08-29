use crate::{BASE_URL, net::Url};
use aidoku::{
	Manga, MangaPageResult, MangaStatus, Page, Result, Viewer,
	alloc::{String, Vec, string::ToString as _, vec},
	error,
	imports::html::{Document, Element, ElementList},
	prelude::*,
};
use regex::Regex;

fn extract_chapter_number(title: &str) -> Option<f32> {
	// This handles cases like "第183话 180" where 180 is the actual chapter
	let re1 =
		Regex::new(r"(?:第\s*\d+(?:\.\d+)?\s*(?:话|話|章|回|卷|册|冊)\s*)(\d+(?:\.\d+)?)").ok()?;
	if let Some(captures) = re1.captures(title)
		&& let Some(num_match) = captures.get(1)
		&& let Ok(num) = num_match.as_str().parse::<f32>()
	{
		return Some(num);
	}

	// Second try: match "第X话" pattern where X is the chapter number
	let re2 = Regex::new(r"(?:第\s*)(\d+(?:\.\d+)?)\s*(?:话|話|章|回|卷|册|冊)").ok()?;
	if let Some(captures) = re2.captures(title)
		&& let Some(num_match) = captures.get(1)
		&& let Ok(num) = num_match.as_str().parse::<f32>()
	{
		return Some(num);
	}

	// Third try: match pure number at the beginning
	let re3 = Regex::new(r"^(\d+(?:\.\d+)?)").ok()?;
	if let Some(captures) = re3.captures(title)
		&& let Some(num_match) = captures.get(1)
		&& let Ok(num) = num_match.as_str().parse::<f32>()
	{
		return Some(num);
	}

	None
}

pub trait MangaPage {
	fn update_details(&self, manga: &mut Manga) -> Result<()>;
	fn manga_page_result(&self) -> Result<MangaPageResult>;
}

impl MangaPage for Document {
	fn update_details(&self, manga: &mut Manga) -> Result<()> {
		// Extract cover from meta tag or resize URL
		manga.cover = self
			.select_first("meta[name='og:image']")
			.and_then(|meta| meta.attr("content"))
			.or_else(|| {
				self.select_first("amp-img.comic-cover")
					.and_then(|img| img.attr("src"))
			});

		// Remove query params from cover if exists
		if let Some(ref cover) = manga.cover
			&& let Some(pos) = cover.rfind('?')
		{
			manga.cover = Some(cover[..pos].to_string());
		}

		manga.title = self
			.select_first("meta[name='og:novel:book_name']")
			.and_then(|meta| meta.attr("content"))
			.unwrap_or_default();

		let author = self
			.select_first("meta[name='og:novel:author']")
			.and_then(|meta| meta.attr("content"))
			.unwrap_or_default();

		// Deduplicate and join artists
		let mut artists: Vec<String> = author
			.split(',')
			.map(|s| s.trim().to_string())
			.filter(|s| !s.is_empty())
			.collect();
		artists.dedup();
		let artist_str = artists.join(", ");

		manga.authors = Some(vec![artist_str]);

		// Extract description
		manga.description = self
			.select_first("meta[name='og:description']")
			.and_then(|meta| meta.attr("content"))
			.map(|desc| {
				// Remove prefix if exists
				if let Some(pos) = desc.find("》全集，") {
					desc[pos + "》全集，".len()..].trim().to_string()
				} else {
					desc
				}
			});

		// Extract categories/tags
		let tags = self
			.try_select("span.tag")?
			.skip(1) // Skip first tag (usually status)
			.filter_map(|tag| tag.text())
			.filter(|t| !t.is_empty())
			.collect::<Vec<String>>();
		manga.tags = Some(tags);

		let tags = manga.tags.as_deref().unwrap_or(&[]);
		manga.viewer = if tags
			.iter()
			.any(|tag| tag.contains("國漫") || tag.contains("韓國"))
		{
			Viewer::Webtoon
		} else if tags.iter().any(|tag| tag.contains("日本")) {
			Viewer::RightToLeft
		} else {
			Viewer::LeftToRight
		};

		// Extract status
		let status_str = self
			.select_first("meta[name='og:novel:status']")
			.and_then(|meta| meta.attr("content"))
			.unwrap_or_default();
		manga.status = match status_str.as_str() {
			"連載中" | "连载中" => MangaStatus::Ongoing,
			"已完結" | "已完结" => MangaStatus::Completed,
			_ => MangaStatus::Unknown,
		};

		manga.url = Some(Url::manga(manga.key.clone()).to_string());
		Ok(())
	}

	fn manga_page_result(&self) -> Result<MangaPageResult> {
		let mut entries: Vec<Manga> = Vec::new();

		for item in self.try_select("div.comics-card")? {
			let url = item
				.select_first("a.comics-card__poster")
				.and_then(|a| a.attr("href"))
				.unwrap_or_default();

			let Some(key) = url
				.split('/')
				.rfind(|s| !s.is_empty())
				.map(|s| s.to_string())
			else {
				continue;
			};

			let cover = item
				.select_first("amp-img[noloading]")
				.and_then(|img| img.attr("src"))
				.map(|src| {
					// Remove query params
					if let Some(pos) = src.rfind('?') {
						src[..pos].to_string()
					} else {
						src
					}
				});

			let title = item
				.select_first("h3")
				.and_then(|h3| h3.text())
				.unwrap_or_default();

			let artist = item
				.select_first("small")
				.and_then(|small| small.text())
				.map(|text| {
					let mut artists: Vec<String> = text
						.split(',')
						.map(|s| s.trim().to_string())
						.filter(|s| !s.is_empty())
						.collect();
					artists.dedup();
					artists.join(", ")
				})
				.unwrap_or_default();

			let tags = item
				.select("span")
				.map(|spans| {
					spans
						.filter_map(|span| span.text())
						.filter(|t| !t.is_empty())
						.collect::<Vec<String>>()
				})
				.unwrap_or_default();

			entries.push(Manga {
				key,
				cover,
				title,
				authors: Some(vec![artist]),
				url: Some(format!("{}{}", BASE_URL, url)),
				tags: Some(tags),
				..Default::default()
			});
		}

		Ok(MangaPageResult {
			entries,
			has_next_page: false,
		})
	}
}

pub trait ChapterPage {
	fn chapters(&self, manga_id: &str) -> Result<Vec<aidoku::Chapter>>;
}

fn app_chapter_parts(onclick: &str) -> Option<(String, String, String)> {
	let re = Regex::new(
		r"send_app_msg\('call_page',\s*\['chapter',\s*'([^']+)',\s*(\d+),\s*(\d+)\]\)",
	)
	.ok()?;
	let captures = re.captures(onclick)?;
	Some((
		captures.get(1)?.as_str().to_string(),
		captures.get(2)?.as_str().to_string(),
		captures.get(3)?.as_str().to_string(),
	))
}

fn chapter_key(href: &str) -> String {
	let section = href
		.split("section_slot=")
		.nth(1)
		.and_then(|v| v.split('&').next())
		.filter(|v| !v.is_empty());
	let chapter = href
		.split("chapter_slot=")
		.nth(1)
		.and_then(|v| v.split('&').next())
		.filter(|v| !v.is_empty());
	match (section, chapter) {
		(Some(section), Some(chapter)) => format!("{}_{}", section, chapter),
		_ => href
			.split('?')
			.next()
			.unwrap_or(href)
			.rsplit('/')
			.next()
			.unwrap_or("")
			.trim_end_matches(".html")
			.to_string(),
	}
}

fn comic_id_from_href(href: &str) -> Option<String> {
	href
		.split("comic_id=")
		.nth(1)
		.and_then(|v| v.split('&').next())
		.filter(|v| !v.is_empty())
		.map(|v| v.to_string())
}

impl ChapterPage for Document {
	fn chapters(&self, manga_id: &str) -> Result<Vec<aidoku::Chapter>> {
		let full_list_title = self.select(".section-title").and_then(|items| {
			items
				.filter_map(|el| el.text().map(|t| (el, t)))
				.find(|(_, t)| t.contains("章节目录") || t.contains("章節目錄"))
				.map(|(el, _)| el)
		});

		let raw_items: Vec<Element> = if let Some(ref title_el) = full_list_title {
			title_el
				.parent()
				.and_then(|parent| parent.select("div.pure-g[id] a.comics-chapters__item, div.pure-g[id] div.comics-chapters"))
				.into_iter()
				.flatten()
				.collect()
		} else {
			self.select("a.comics-chapters__item, div.comics-chapters")
				.into_iter()
				.flatten()
				.collect()
		};

		let mut chapters: Vec<aidoku::Chapter> = Vec::new();
		let mut index = 0.0;

		for item in &raw_items {
			index += 1.0;
			let url = item.attr("href").unwrap_or_default();
			let onclick = item
				.attr("onclick")
				.or_else(|| item.select_first("[onclick]").and_then(|el| el.attr("onclick")))
				.unwrap_or_default();
			let (key, chapter_url) = if !url.is_empty() {
				let key = chapter_key(&url);
				let comic_id = comic_id_from_href(&url).unwrap_or_else(|| manga_id.to_string());
				let chapter_url = if key.is_empty() {
					String::new()
				} else {
					Url::chapter(comic_id, key.clone()).to_string()
				};
				(key, chapter_url)
			} else if let Some((slug, section, chapter)) = app_chapter_parts(&onclick) {
				let key = format!("{}_{}", section, chapter);
				let chapter_url = Url::chapter(slug, key.clone()).to_string();
				(key, chapter_url)
			} else {
				(String::new(), String::new())
			};

			let title = item.text().unwrap_or_default();

			let chapter_or_volume = extract_chapter_number(&title).unwrap_or(index);
			let (ch, vo) = if title.trim().ends_with('卷') {
				(-1.0, chapter_or_volume)
			} else {
				(chapter_or_volume, -1.0)
			};

			let scanlator = if vo > -1.0 {
				"单行本".to_string()
			} else {
				"默认".to_string()
			};

			if key.is_empty() {
				continue;
			}

			chapters.push(aidoku::Chapter {
				key,
				title: Some(title),
				volume_number: (vo >= 0.0).then_some(vo),
				chapter_number: (ch >= 0.0).then_some(ch),
				url: (!chapter_url.is_empty()).then_some(chapter_url),
				scanlators: Some(vec![scanlator]),
				..Default::default()
			});
		}

		if full_list_title.is_some() {
			chapters.reverse();
		}

		if let Some(first) = chapters.first_mut() {
			first.key = format!("{}|latest", first.key);
		}

		if chapters.is_empty() {
			bail!("No chapters found");
		}

		Ok(chapters)
	}
}

pub trait PageList {
	fn pages(&self) -> Result<Vec<Page>>;
}

impl PageList for Document {
	fn pages(&self) -> Result<Vec<Page>> {
		let items = self.try_select(
			".comic-contain img, .comic-article img, .chapter-img img, .comic-page img, img.comic-contain__item",
		)?;

		let pages = items
			.filter_map(|item| {
				let url = item
					.attr("data-src")
					.or_else(|| item.attr("src"))
					.unwrap_or_default();
				if url.is_empty() {
					None
				} else if !url.contains("/scomic/") && !url.contains("comic") {
					None
				} else {
					Some(Page {
						content: aidoku::PageContent::url(url),
						..Default::default()
					})
				}
			})
			.collect();

		Ok(pages)
	}
}

trait TrySelect {
	fn try_select<S: AsRef<str>>(&self, css_query: S) -> Result<ElementList>;
}

impl TrySelect for Document {
	fn try_select<S: AsRef<str>>(&self, css_query: S) -> Result<ElementList> {
		self.select(&css_query)
			.ok_or_else(|| error!("No element found for selector: `{}`", css_query.as_ref()))
	}
}

impl TrySelect for Element {
	fn try_select<S: AsRef<str>>(&self, css_query: S) -> Result<ElementList> {
		self.select(&css_query)
			.ok_or_else(|| error!("No element found for selector: `{}`", css_query.as_ref()))
	}
}
