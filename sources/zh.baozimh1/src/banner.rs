use aidoku::{
	ImageResponse,
	alloc::Vec,
	imports::canvas::{Canvas, ImageRef, Rect},
};
use png::{BitDepth, ColorType, Decoder, Transformations};

use crate::banner_signatures::{BANNER_SIGNATURE, NARROW_BANNER_SIGNATURE, OLD_BANNER_SIGNATURE};

const BANNER_WIDTH: usize = 800;
const BANNER_HEIGHT: usize = 200;
const OLD_BANNER_HEIGHT: usize = 282;
const NARROW_BANNER_WIDTH: usize = 640;
const NARROW_BANNER_HEIGHT: usize = 186;
const NARROW_BOTTOM_HEIGHT: usize = 160;
const SIGNATURE_COLS: usize = 16;
const MAX_AVG_ERROR_PER_COMPONENT: usize = 6;

struct RowSamples {
	wide: Option<[u8; 3]>,
	narrow: [u8; 3],
}

enum Signature<'a> {
	Fixed(&'a [[u8; 3]]),
	ScaledNarrow { height: usize, full_height: usize },
}

impl Signature<'_> {
	fn height(&self) -> usize {
		match self {
			Self::Fixed(rows) => rows.len(),
			Self::ScaledNarrow { height, .. } => *height,
		}
	}

	fn row(&self, y: usize) -> [u8; 3] {
		match self {
			Self::Fixed(rows) => rows[y],
			Self::ScaledNarrow { full_height, .. } => scaled_narrow_row(y, *full_height),
		}
	}
}

pub fn process_image(response: ImageResponse) -> ImageRef {
	let width = response.image.width().round() as usize;
	let height = response.image.height().round() as usize;
	if width < NARROW_BANNER_WIDTH || height <= NARROW_BOTTOM_HEIGHT {
		return response.image;
	}

	let raw_data = response.image.data();
	let data = if raw_data.starts_with(b"\x89PNG\r\n\x1a\n") {
		raw_data
	} else {
		// Network responses are usually stored as their original JPEG/WebP bytes.
		// Round-trip through the host image API so the Rust side only needs one
		// small streaming PNG decoder.
		ImageRef::new(&raw_data).data()
	};
	let Some(rows) = decode_row_samples(&data, width, height) else {
		return response.image;
	};

	let (top, bottom) = crop_amounts(&rows, width, height);
	let content_height = height.saturating_sub(top + bottom);
	if (top == 0 && bottom == 0) || content_height == 0 {
		return response.image;
	}

	let mut canvas = Canvas::new(width as f32, content_height as f32);
	canvas.copy_image(
		&response.image,
		Rect::new(0.0, top as f32, width as f32, content_height as f32),
		Rect::new(0.0, 0.0, width as f32, content_height as f32),
	);
	canvas.get_image()
}

fn decode_row_samples(
	data: &[u8],
	expected_width: usize,
	expected_height: usize,
) -> Option<Vec<RowSamples>> {
	let mut decoder = Decoder::new(data);
	decoder.set_transformations(Transformations::EXPAND | Transformations::STRIP_16);
	let mut reader = decoder.read_info().ok()?;
	let info = reader.info();
	let width = info.width as usize;
	let height = info.height as usize;
	if width != expected_width || height != expected_height || info.interlaced {
		return None;
	}

	let (color_type, bit_depth) = reader.output_color_type();
	if bit_depth != BitDepth::Eight {
		return None;
	}

	let narrow_width = if width < BANNER_WIDTH {
		width
	} else {
		NARROW_BANNER_WIDTH
	};
	let narrow_x = (width - narrow_width) / 2;
	let wide_x = (width >= BANNER_WIDTH).then_some((width - BANNER_WIDTH) / 2);
	let mut rows = Vec::with_capacity(height);

	while let Some(row) = reader.next_row().ok()? {
		let data = row.data();
		rows.push(RowSamples {
			wide: wide_x.map(|x| average_row(data, color_type, x, BANNER_WIDTH)),
			narrow: average_row(data, color_type, narrow_x, narrow_width),
		});
	}

	(rows.len() == height).then_some(rows)
}

fn average_row(data: &[u8], color_type: ColorType, x: usize, width: usize) -> [u8; 3] {
	let start = width / 5;
	let span = width * 3 / 5;
	let mut sums = [0usize; 3];

	for index in 0..SIGNATURE_COLS {
		let sample_x = x + start + index * span / SIGNATURE_COLS;
		let color = pixel(data, color_type, sample_x);
		for component in 0..3 {
			sums[component] += color[component] as usize;
		}
	}

	[
		(sums[0] / SIGNATURE_COLS) as u8,
		(sums[1] / SIGNATURE_COLS) as u8,
		(sums[2] / SIGNATURE_COLS) as u8,
	]
}

fn pixel(data: &[u8], color_type: ColorType, x: usize) -> [u8; 3] {
	match color_type {
		ColorType::Rgb => {
			let offset = x * 3;
			[data[offset], data[offset + 1], data[offset + 2]]
		}
		ColorType::Rgba => {
			let offset = x * 4;
			[data[offset], data[offset + 1], data[offset + 2]]
		}
		ColorType::Grayscale => {
			let value = data[x];
			[value, value, value]
		}
		ColorType::GrayscaleAlpha => {
			let value = data[x * 2];
			[value, value, value]
		}
		ColorType::Indexed => [0, 0, 0],
	}
}

fn crop_amounts(rows: &[RowSamples], width: usize, height: usize) -> (usize, usize) {
	let narrow_height = scaled_height(NARROW_BANNER_HEIGHT, width);
	let narrow_bottom_height = scaled_height(NARROW_BOTTOM_HEIGHT, width);
	let narrow_top = if width < BANNER_WIDTH {
		Signature::ScaledNarrow {
			height: narrow_height,
			full_height: narrow_height,
		}
	} else {
		Signature::Fixed(&NARROW_BANNER_SIGNATURE)
	};
	let narrow_bottom = if width < BANNER_WIDTH {
		Signature::ScaledNarrow {
			height: narrow_bottom_height,
			full_height: narrow_height,
		}
	} else {
		Signature::Fixed(&NARROW_BANNER_SIGNATURE[..NARROW_BOTTOM_HEIGHT])
	};

	let mut top = 0;
	while top < height {
		let consumed = match_top(rows, top, &narrow_top);
		if consumed == 0 {
			break;
		}
		top += consumed;
	}

	let mut bottom = 0;
	while top + bottom < height {
		let consumed = match_bottom(rows, top, bottom, &narrow_bottom);
		if consumed == 0 {
			break;
		}
		bottom += consumed;
	}

	(top, bottom)
}

fn match_top(rows: &[RowSamples], y: usize, narrow: &Signature<'_>) -> usize {
	if matches(rows, y, &Signature::Fixed(&BANNER_SIGNATURE), true) {
		return BANNER_HEIGHT;
	}
	if matches(rows, y, &Signature::Fixed(&OLD_BANNER_SIGNATURE), true) {
		return OLD_BANNER_HEIGHT;
	}
	if matches(rows, y, narrow, false) {
		return narrow.height();
	}
	0
}

fn match_bottom(rows: &[RowSamples], top: usize, bottom: usize, narrow: &Signature<'_>) -> usize {
	for (signature, wide) in [
		(Signature::Fixed(&OLD_BANNER_SIGNATURE), true),
		(Signature::Fixed(&BANNER_SIGNATURE), true),
	] {
		let signature_height = signature.height();
		if let Some(y) = rows.len().checked_sub(bottom + signature_height)
			&& y >= top
			&& matches(rows, y, &signature, wide)
		{
			return signature_height;
		}
	}

	let signature_height = narrow.height();
	if let Some(y) = rows.len().checked_sub(bottom + signature_height)
		&& y >= top
		&& matches(rows, y, narrow, false)
	{
		return signature_height;
	}
	0
}

fn matches(rows: &[RowSamples], y: usize, signature: &Signature<'_>, wide: bool) -> bool {
	let height = signature.height();
	if y + height > rows.len() {
		return false;
	}

	let threshold = height * 3 * MAX_AVG_ERROR_PER_COMPONENT;
	let mut difference = 0usize;
	for row in 0..height {
		let actual = if wide {
			let Some(actual) = rows[y + row].wide else {
				return false;
			};
			actual
		} else {
			rows[y + row].narrow
		};
		let expected = signature.row(row);
		for component in 0..3 {
			difference += actual[component].abs_diff(expected[component]) as usize;
		}
		if difference > threshold {
			return false;
		}
	}
	true
}

fn scaled_height(base_height: usize, width: usize) -> usize {
	if width >= BANNER_WIDTH {
		base_height
	} else {
		(base_height * width + NARROW_BANNER_WIDTH / 2) / NARROW_BANNER_WIDTH
	}
}

fn scaled_narrow_row(y: usize, height: usize) -> [u8; 3] {
	if height == NARROW_BANNER_HEIGHT {
		return NARROW_BANNER_SIGNATURE[y];
	}

	let source_position = ((y as f32 + 0.5) * NARROW_BANNER_HEIGHT as f32 / height as f32 - 0.5)
		.clamp(0.0, (NARROW_BANNER_HEIGHT - 1) as f32);
	let first = source_position as usize;
	let second = (first + 1).min(NARROW_BANNER_HEIGHT - 1);
	let weight = source_position - first as f32;
	let mut result = [0u8; 3];
	for component in 0..3 {
		let start = NARROW_BANNER_SIGNATURE[first][component] as f32;
		let end = NARROW_BANNER_SIGNATURE[second][component] as f32;
		result[component] = (start + (end - start) * weight).round() as u8;
	}
	result
}
