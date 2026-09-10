use aidoku::{
	alloc::Vec,
	imports::canvas::{Canvas, ImageRef, Rect},
};
use miniz_oxide::{
	DataFormat, MZError, MZFlush, MZStatus,
	inflate::stream::{InflateState, inflate},
};

const ROW_SAMPLES: usize = 32;
const SPLIT_BAND_RADIUS: usize = 2;
const MAX_AVG_SPLIT_ERROR: usize = 8;
const MAX_AVG_IMAGE_ERROR: usize = 12;

type RowSamples = [[u8; 3]; ROW_SAMPLES];

pub fn merge_images(primary: ImageRef, alternate: ImageRef) -> ImageRef {
	let width = primary.width() as usize;
	let height = primary.height() as usize;
	if width == 0
		|| height < 4
		|| alternate.width() as usize != width
		|| alternate.height() as usize != height
	{
		return primary;
	}

	let Some(primary_rows) = image_row_samples(&primary, width, height) else {
		return primary;
	};
	let Some(alternate_rows) = image_row_samples(&alternate, width, height) else {
		return primary;
	};
	let Some(split) = best_split(&primary_rows, &alternate_rows) else {
		return primary;
	};

	let mut canvas = Canvas::new(width as f32, height as f32);
	canvas.copy_image(
		&primary,
		Rect::new(0.0, 0.0, width as f32, split as f32),
		Rect::new(0.0, 0.0, width as f32, split as f32),
	);
	canvas.copy_image(
		&alternate,
		Rect::new(
			0.0,
			split as f32,
			width as f32,
			(height - split) as f32,
		),
		Rect::new(
			0.0,
			split as f32,
			width as f32,
			(height - split) as f32,
		),
	);
	canvas.get_image()
}

fn image_row_samples(image: &ImageRef, width: usize, height: usize) -> Option<Vec<RowSamples>> {
	let raw_data = image.data();
	// ImageRef data may still contain the original JPEG/WebP response. Re-importing
	// it makes the host return a predictable PNG stream for row comparison.
	let png_data = ImageRef::new(&raw_data).data();
	decode_row_samples(&png_data, width, height)
}

fn best_split(primary: &[RowSamples], alternate: &[RowSamples]) -> Option<usize> {
	if primary.len() != alternate.len() || primary.len() < 4 {
		return None;
	}
	if image_error(primary, alternate) > MAX_AVG_IMAGE_ERROR {
		return None;
	}

	let height = primary.len();
	let start = (height / 4).max(SPLIT_BAND_RADIUS);
	let end = (height * 3 / 4).min(height - SPLIT_BAND_RADIUS - 1);
	if start > end {
		return None;
	}

	let mut best = (usize::MAX, start);
	for y in start..=end {
		let error = band_error(primary, alternate, y);
		if error < best.0 {
			best = (error, y);
		}
	}

	(best.0 <= MAX_AVG_SPLIT_ERROR).then_some(best.1)
}

fn image_error(primary: &[RowSamples], alternate: &[RowSamples]) -> usize {
	let mut difference = 0usize;
	let mut components = 0usize;
	for row in 0..primary.len() {
		for sample in 0..ROW_SAMPLES {
			for component in 0..3 {
				difference += primary[row][sample][component]
					.abs_diff(alternate[row][sample][component]) as usize;
				components += 1;
			}
		}
	}
	difference / components
}

fn band_error(primary: &[RowSamples], alternate: &[RowSamples], y: usize) -> usize {
	let mut difference = 0usize;
	let mut components = 0usize;
	for row in (y - SPLIT_BAND_RADIUS)..=(y + SPLIT_BAND_RADIUS) {
		for sample in 0..ROW_SAMPLES {
			for component in 0..3 {
				difference += primary[row][sample][component]
					.abs_diff(alternate[row][sample][component]) as usize;
				components += 1;
			}
		}
	}
	difference / components
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
					png.width,
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
				png.width,
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
	width: usize,
	rows: &mut Vec<RowSamples>,
) -> Option<()> {
	unfilter_scanline(row, previous, channels)?;
	let pixels = &row[1..];
	let mut samples = [[0u8; 3]; ROW_SAMPLES];
	for (index, sample) in samples.iter_mut().enumerate() {
		let x = ((index * 2 + 1) * width / (ROW_SAMPLES * 2)).min(width - 1);
		*sample = pixel(pixels, channels, x);
	}
	rows.push(samples);
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

fn pixel(data: &[u8], channels: usize, x: usize) -> [u8; 3] {
	let offset = x * channels;
	if channels < 3 {
		[data[offset], data[offset], data[offset]]
	} else {
		[data[offset], data[offset + 1], data[offset + 2]]
	}
}
