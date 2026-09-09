use aidoku::{
	ImageResponse,
	alloc::Vec,
	imports::canvas::{Canvas, ImageRef, Rect},
};
use miniz_oxide::{
	DataFormat, MZError, MZFlush, MZStatus,
	inflate::stream::{InflateState, inflate},
};

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
	let width = response.image.width() as usize;
	let height = response.image.height() as usize;
	if width < NARROW_BANNER_WIDTH || height <= NARROW_BOTTOM_HEIGHT {
		return response.image;
	}

	let raw_data = response.image.data();
	// Network responses are usually stored as their original JPEG/WebP bytes.
	// Round-trip through the host image API to get a predictable PNG stream.
	let data = ImageRef::new(&raw_data).data();
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
	let png = parse_png(data)?;
	if png.width != expected_width || png.height != expected_height {
		return None;
	}

	let narrow_width = if png.width < BANNER_WIDTH {
		png.width
	} else {
		NARROW_BANNER_WIDTH
	};
	let narrow_x = (png.width - narrow_width) / 2;
	let wide_x = (png.width >= BANNER_WIDTH).then_some((png.width - BANNER_WIDTH) / 2);
	let row_size = png.width.checked_mul(png.channels)?.checked_add(1)?;
	let mut scanline = Vec::new();
	scanline.resize(row_size, 0u8);
	let mut previous = Vec::new();
	previous.resize(row_size - 1, 0u8);
	let mut filled = 0usize;
	let mut rows = Vec::with_capacity(png.height);
	let mut inflater = InflateState::new_boxed(DataFormat::Zlib);
	let mut stream_ended = false;

	for chunk in png.idat_chunks {
		let mut input = chunk;
		while !input.is_empty() {
			let result = inflate(&mut inflater, input, &mut scanline[filled..], MZFlush::None);
			input = &input[result.bytes_consumed..];
			filled += result.bytes_written;

			if filled == row_size {
				push_scanline(
					&mut scanline,
					&mut previous,
					png.channels,
					wide_x,
					narrow_x,
					narrow_width,
					&mut rows,
				)?;
				filled = 0;
			}

			match result.status {
				Ok(MZStatus::StreamEnd) => {
					stream_ended = true;
					break;
				}
				Ok(MZStatus::Ok) => {}
				Ok(MZStatus::NeedDict) | Err(MZError::Data | MZError::Stream | MZError::Param) => {
					return None;
				}
				Err(MZError::Buf) if result.bytes_consumed > 0 || result.bytes_written > 0 => {}
				Err(_) => return None,
			}

			if result.bytes_consumed == 0 && result.bytes_written == 0 {
				return None;
			}
		}
		if stream_ended {
			break;
		}
	}

	while !stream_ended {
		let result = inflate(&mut inflater, &[], &mut scanline[filled..], MZFlush::Finish);
		filled += result.bytes_written;
		if filled == row_size {
			push_scanline(
				&mut scanline,
				&mut previous,
				png.channels,
				wide_x,
				narrow_x,
				narrow_width,
				&mut rows,
			)?;
			filled = 0;
		}

		match result.status {
			Ok(MZStatus::StreamEnd) => stream_ended = true,
			Ok(MZStatus::Ok) | Err(MZError::Buf) if result.bytes_written > 0 => {}
			_ => return None,
		}
	}

	(stream_ended && filled == 0 && rows.len() == png.height).then_some(rows)
}

fn push_scanline(
	row: &mut [u8],
	previous: &mut [u8],
	channels: usize,
	wide_x: Option<usize>,
	narrow_x: usize,
	narrow_width: usize,
	rows: &mut Vec<RowSamples>,
) -> Option<()> {
	unfilter_scanline(row, previous, channels)?;
	let pixels = &row[1..];
	rows.push(RowSamples {
		wide: wide_x.map(|x| average_row(pixels, channels, x, BANNER_WIDTH)),
		narrow: average_row(pixels, channels, narrow_x, narrow_width),
	});
	previous.copy_from_slice(pixels);
	Some(())
}

struct PngData<'a> {
	width: usize,
	height: usize,
	channels: usize,
	idat_chunks: Vec<&'a [u8]>,
}

fn parse_png(data: &[u8]) -> Option<PngData<'_>> {
	if !data.starts_with(b"\x89PNG\r\n\x1a\n") {
		return None;
	}

	let mut offset = 8usize;
	let mut width = 0usize;
	let mut height = 0usize;
	let mut channels = 0usize;
	let mut idat_chunks = Vec::new();
	while offset.checked_add(12)? <= data.len() {
		let length = read_u32(&data[offset..offset + 4])? as usize;
		let chunk_end = offset.checked_add(12)?.checked_add(length)?;
		if chunk_end > data.len() {
			return None;
		}
		let kind = &data[offset + 4..offset + 8];
		let chunk = &data[offset + 8..offset + 8 + length];
		match kind {
			b"IHDR" if length == 13 => {
				width = read_u32(&chunk[0..4])? as usize;
				height = read_u32(&chunk[4..8])? as usize;
				if chunk[8] != 8 || chunk[10] != 0 || chunk[11] != 0 || chunk[12] != 0 {
					return None;
				}
				channels = match chunk[9] {
					0 => 1,
					2 => 3,
					4 => 2,
					6 => 4,
					_ => return None,
				};
			}
			b"IDAT" => idat_chunks.push(chunk),
			b"IEND" => break,
			_ => {}
		}
		offset = chunk_end;
	}

	(width > 0 && height > 0 && channels > 0 && !idat_chunks.is_empty()).then_some(PngData {
		width,
		height,
		channels,
		idat_chunks,
	})
}

fn read_u32(bytes: &[u8]) -> Option<u32> {
	Some(u32::from_be_bytes(bytes.try_into().ok()?))
}

fn unfilter_scanline(row: &mut [u8], previous: &[u8], bytes_per_pixel: usize) -> Option<()> {
	let filter = row[0];
	for index in 0..previous.len() {
		let left = if index >= bytes_per_pixel {
			row[index + 1 - bytes_per_pixel]
		} else {
			0
		};
		let above = previous[index];
		let upper_left = if index >= bytes_per_pixel {
			previous[index - bytes_per_pixel]
		} else {
			0
		};
		let prediction = match filter {
			0 => 0,
			1 => left,
			2 => above,
			3 => ((left as u16 + above as u16) / 2) as u8,
			4 => paeth(left, above, upper_left),
			_ => return None,
		};
		row[index + 1] = row[index + 1].wrapping_add(prediction);
	}
	Some(())
}

fn paeth(left: u8, above: u8, upper_left: u8) -> u8 {
	let left = left as i32;
	let above = above as i32;
	let upper_left = upper_left as i32;
	let estimate = left + above - upper_left;
	let left_distance = (estimate - left).unsigned_abs();
	let above_distance = (estimate - above).unsigned_abs();
	let upper_left_distance = (estimate - upper_left).unsigned_abs();
	if left_distance <= above_distance && left_distance <= upper_left_distance {
		left as u8
	} else if above_distance <= upper_left_distance {
		above as u8
	} else {
		upper_left as u8
	}
}

fn average_row(data: &[u8], channels: usize, x: usize, width: usize) -> [u8; 3] {
	let start = width / 5;
	let span = width * 3 / 5;
	let mut sums = [0usize; 3];

	for index in 0..SIGNATURE_COLS {
		let sample_x = x + start + index * span / SIGNATURE_COLS;
		let color = pixel(data, channels, sample_x);
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

fn pixel(data: &[u8], channels: usize, x: usize) -> [u8; 3] {
	let offset = x * channels;
	if channels < 3 {
		[data[offset], data[offset], data[offset]]
	} else {
		[data[offset], data[offset + 1], data[offset + 2]]
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
		result[component] = (start + (end - start) * weight + 0.5) as u8;
	}
	result
}
