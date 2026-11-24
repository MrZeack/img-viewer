use crate::errors::AppError;
use chrono::{DateTime, Local, TimeZone};
use exif::Value;
use memmap2::Mmap;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fs::File;
use std::io::{BufReader, Cursor};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageMetadata {
    pub file_info: FileInfo,
    pub image_info: ImageInfo,
    pub exif_info: Option<ExifInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub name: String,
    pub size: u64,
    pub created: Option<DateTime<Local>>,
    pub modified: Option<DateTime<Local>>,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInfo {
    pub dimensions: (u32, u32),
    pub megapixels: f64,
    pub aspect_ratio: String,
    pub format: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExifInfo {
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub date_taken: Option<DateTime<Local>>,
    pub shutter_speed: Option<String>,
    pub aperture: Option<String>,
    pub iso: Option<u32>,
    pub focal_length: Option<String>,
    pub flash: Option<String>,
    pub lens_model: Option<String>,
    pub gps_location: Option<String>,
    pub color_space: Option<String>,
    pub metering_mode: Option<String>,
    pub exposure_bias_value: Option<String>,
    pub aperture_value: Option<String>,
    pub sensitivity_type: Option<String>,
    pub exposure_program: Option<String>,
    pub orientation: Option<String>,
    /// Numeric EXIF Orientation tag (1..=8), used directly for auto-rotation.
    pub orientation_code: Option<u32>,
}

pub fn extract_metadata(path: &Path) -> Result<ImageMetadata, AppError> {
    let metadata = std::fs::metadata(path).map_err(|e| {
        AppError::MetadataError(format!(
            "Failed to get file metadata for {}: {}",
            path.display(),
            e
        ))
    })?;

    let file_info = FileInfo {
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        size: metadata.len(),
        created: metadata.created().ok().and_then(system_time_to_local),
        modified: metadata.modified().ok().and_then(system_time_to_local),
        path: path
            .parent()
            .unwrap_or(Path::new(""))
            .to_string_lossy()
            .to_string(),
    };

    let mmap = if file_info.size > 0 {
        File::open(path)
            .ok()
            .and_then(|file| unsafe { Mmap::map(&file).ok() })
    } else {
        None
    };
    let file_data_slice = mmap.as_deref();

    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    let (dimensions, exif_info) = match ext.as_str() {
        "hif" | "heic" => extract_heif_dimensions_and_exif(path, file_data_slice),
        "arw" | "cr3" => (
            extract_raw_dimensions(file_data_slice),
            extract_raw_exif_data(path, file_data_slice),
        ),
        _ => (
            image::image_dimensions(path).unwrap_or((0, 0)),
            extract_exif_data_from_file(path),
        ),
    };

    let megapixels = (dimensions.0 as f64 * dimensions.1 as f64) / 1_000_000.0;
    let aspect_ratio = if dimensions.0 > 0 && dimensions.1 > 0 {
        let gcd_val = gcd(dimensions.0, dimensions.1);
        format!("{}:{}", dimensions.0 / gcd_val, dimensions.1 / gcd_val)
    } else {
        "—".to_string()
    };

    let image_info = ImageInfo {
        dimensions,
        megapixels,
        aspect_ratio,
        format: guess_format_from_path(path),
    };

    Ok(ImageMetadata {
        file_info,
        image_info,
        exif_info,
    })
}

fn system_time_to_local(t: std::time::SystemTime) -> Option<DateTime<Local>> {
    let duration = t.duration_since(std::time::SystemTime::UNIX_EPOCH).ok()?;
    DateTime::from_timestamp(duration.as_secs() as i64, duration.subsec_nanos())
        .map(|dt| dt.with_timezone(&Local))
}

/// Parses HEIF container once to read both primary image dimensions and EXIF block.
fn extract_heif_dimensions_and_exif(
    path: &Path,
    data_slice: Option<&[u8]>,
) -> ((u32, u32), Option<ExifInfo>) {
    use libheif_rs::HeifContext;

    let owned;
    let data = match data_slice {
        Some(d) => d,
        None => match std::fs::read(path) {
            Ok(d) => {
                owned = d;
                &owned
            }
            Err(_) => return ((0, 0), None),
        },
    };
    if data.is_empty() {
        return ((0, 0), None);
    }

    let Ok(ctx) = HeifContext::read_from_bytes(data) else {
        return ((0, 0), None);
    };
    let Ok(handle) = ctx.primary_image_handle() else {
        return ((0, 0), None);
    };

    let dims = (handle.width(), handle.height());

    let mut item_ids = [libheif_rs::ItemId::default(); 1];
    let count = handle.metadata_block_ids(&mut item_ids, b"Exif");
    if count == 0 {
        return (dims, None);
    }
    let Ok(exif_data) = handle.metadata(item_ids[0]) else {
        return (dims, None);
    };

    let exif_bytes = if exif_data.len() > 4 {
        &exif_data[4..]
    } else {
        &exif_data
    };

    let exif_reader = exif::Reader::new();
    let exif_info = exif_reader
        .read_from_container(&mut Cursor::new(exif_bytes))
        .ok()
        .or_else(|| {
            let trimmed = if exif_bytes.starts_with(b"Exif\0\0") {
                &exif_bytes[6..]
            } else {
                exif_bytes
            };
            exif_reader
                .read_from_container(&mut Cursor::new(trimmed))
                .ok()
        })
        .and_then(|exif| convert_kamadak_exif_to_exif_info(std::slice::from_ref(&exif)));

    (dims, exif_info)
}

fn extract_raw_dimensions(data_slice: Option<&[u8]>) -> (u32, u32) {
    let Some(data) = data_slice else {
        return (0, 0);
    };
    let is_cr3 = crate::cr3::is_cr3(data);
    let jpg_data = is_cr3
        .then(|| crate::cr3::preview_jpeg(data))
        .flatten()
        .map(|d| (Cow::Borrowed(d), 1))
        .or_else(|| {
            crate::image_loader::safe_export_thumbnail(data)
                .map(|(d, orientation)| (Cow::Owned(d), orientation))
        })
        .or_else(|| {
            crate::thumbnails::generate::bounded_preview_jpeg(data, data.len())
                .map(|d| (Cow::Borrowed(d), 1))
        });

    let cr3_orientation = is_cr3.then(|| crate::cr3::orientation(data)).flatten();

    let Some((jpg_data, orientation)) = jpg_data else {
        return (0, 0);
    };

    let header_dimensions =
        image::ImageReader::with_format(Cursor::new(jpg_data.as_ref()), image::ImageFormat::Jpeg)
            .into_dimensions();
    if let Ok((mut w, mut h)) = header_dimensions {
        if matches!(cr3_orientation.unwrap_or(orientation), 5..=8) {
            std::mem::swap(&mut w, &mut h);
        }
        (w, h)
    } else {
        (0, 0)
    }
}

fn guess_format_from_path(path: &Path) -> String {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_uppercase())
        .unwrap_or_else(|| "Desconocido".to_string())
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

pub fn format_file_size(size: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = size as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", size as u64, UNITS[unit_index])
    } else {
        format!("{:.1} {}", size, UNITS[unit_index])
    }
}

fn extract_raw_exif_data(path: &Path, data_slice: Option<&[u8]>) -> Option<ExifInfo> {
    let owned;
    let data = match data_slice {
        Some(data) => data,
        None => {
            owned = std::fs::read(path).ok()?;
            &owned
        }
    };

    // CR3 is ISO-BMFF: its EXIF lives in the CMT boxes, and the embedded JPEGs carry none.
    if crate::cr3::is_cr3(data) {
        let exifs = crate::cr3::read_exif(data);
        if !exifs.is_empty() {
            return convert_kamadak_exif_to_exif_info(&exifs);
        }
    }

    // TIFF-based RAW files (ARW) can be read directly from the mapped bytes.
    if let Ok(exif) = exif::Reader::new().read_from_container(&mut Cursor::new(data)) {
        if let Some(info) = convert_kamadak_exif_to_exif_info(std::slice::from_ref(&exif)) {
            return Some(info);
        }
    }

    // Last resort: EXIF from the embedded preview.
    let jpg_data = crate::image_loader::safe_export_thumbnail(data)
        .map(|(jpg, _)| Cow::Owned(jpg))
        .or_else(|| {
            crate::thumbnails::generate::bounded_preview_jpeg(data, data.len()).map(Cow::Borrowed)
        })?;
    let exif = exif::Reader::new()
        .read_from_container(&mut Cursor::new(jpg_data.as_ref()))
        .ok()?;
    convert_kamadak_exif_to_exif_info(std::slice::from_ref(&exif))
}

fn extract_exif_data_from_file(path: &Path) -> Option<ExifInfo> {
    let file = File::open(path).ok()?;
    let mut buf_reader = BufReader::new(file);
    let exif = exif::Reader::new()
        .read_from_container(&mut buf_reader)
        .ok()?;
    convert_kamadak_exif_to_exif_info(std::slice::from_ref(&exif))
}

fn convert_kamadak_exif_to_exif_info(exifs: &[exif::Exif]) -> Option<ExifInfo> {
    let mut exif_info = ExifInfo::default();

    if let Some((field, exif)) = find_field(exifs, exif::Tag::Make) {
        exif_info.camera_make = Some(field.display_value().with_unit(exif).to_string());
    }

    if let Some((field, exif)) = find_field(exifs, exif::Tag::Model) {
        exif_info.camera_model = Some(field.display_value().with_unit(exif).to_string());
    }

    if let Some((field, exif)) = find_field(exifs, exif::Tag::DateTimeOriginal) {
        let datetime_str = field.display_value().with_unit(exif).to_string();
        exif_info.date_taken = parse_exif_datetime(&datetime_str);
    }

    if let Some((field, _)) = find_field(exifs, exif::Tag::PhotographicSensitivity) {
        if let Some(iso) = field.value.get_uint(0) {
            exif_info.iso = Some(iso);
        }
    }

    if let Some((field, exif)) = find_field(exifs, exif::Tag::FocalLength) {
        exif_info.focal_length = Some(field.display_value().with_unit(exif).to_string());
    }

    if let Some((field, exif)) = find_field(exifs, exif::Tag::FNumber) {
        exif_info.aperture = Some(field.display_value().with_unit(exif).to_string());
    }

    if let Some((field, exif)) = find_field(exifs, exif::Tag::ExposureTime) {
        exif_info.shutter_speed = Some(field.display_value().with_unit(exif).to_string());
    }

    if let Some((field, exif)) = find_field(exifs, exif::Tag::LensModel) {
        exif_info.lens_model = Some(field.display_value().with_unit(exif).to_string());
    }

    if let Some((field, _)) = find_field(exifs, exif::Tag::Flash) {
        exif_info.flash = Some(format_flash_value(&field.value));
    }

    if let Some((field, _)) = find_field(exifs, exif::Tag::ColorSpace) {
        exif_info.color_space = Some(format_color_space(&field.value));
    }

    if let Some((field, _)) = find_field(exifs, exif::Tag::MeteringMode) {
        exif_info.metering_mode = Some(format_metering_mode(&field.value));
    }

    if let Some((field, exif)) = find_field(exifs, exif::Tag::ExposureBiasValue) {
        exif_info.exposure_bias_value = Some(field.display_value().with_unit(exif).to_string());
    }

    if let Some((field, exif)) = find_field(exifs, exif::Tag::ApertureValue) {
        exif_info.aperture_value = Some(field.display_value().with_unit(exif).to_string());
    }

    if let Some((field, _)) = find_field(exifs, exif::Tag::SensitivityType) {
        exif_info.sensitivity_type = Some(format_sensitivity_type(&field.value));
    }

    if let Some((field, _)) = find_field(exifs, exif::Tag::ExposureProgram) {
        exif_info.exposure_program = Some(format_exposure_program(&field.value));
    }

    if let Some((field, _)) = find_field(exifs, exif::Tag::Orientation) {
        exif_info.orientation_code = field.value.get_uint(0);
        exif_info.orientation = Some(format_orientation(&field.value));
    }

    let lat = find_field(exifs, exif::Tag::GPSLatitude);
    let lon = find_field(exifs, exif::Tag::GPSLongitude);
    if let (Some((lat_field, lat_exif)), Some((lon_field, lon_exif))) = (lat, lon) {
        let lat_str = lat_field.display_value().with_unit(lat_exif).to_string();
        let lon_str = lon_field.display_value().with_unit(lon_exif).to_string();
        let lat_ref = find_field(exifs, exif::Tag::GPSLatitudeRef)
            .map(|(f, e)| f.display_value().with_unit(e).to_string())
            .unwrap_or_default();
        let lon_ref = find_field(exifs, exif::Tag::GPSLongitudeRef)
            .map(|(f, e)| f.display_value().with_unit(e).to_string())
            .unwrap_or_default();
        let lat_part = format!("{lat_str} {lat_ref}").trim().to_string();
        let lon_part = format!("{lon_str} {lon_ref}").trim().to_string();
        exif_info.gps_location = Some(format!("{lat_part}, {lon_part}"));
    }

    Some(exif_info)
}

/// Looks `tag` up in the primary IFD of each `Exif`, returning the field with
/// the `Exif` it belongs to (needed by `with_unit`).
fn find_field(exifs: &[exif::Exif], tag: exif::Tag) -> Option<(&exif::Field, &exif::Exif)> {
    exifs.iter().find_map(|exif| {
        exif.get_field(tag, exif::In::PRIMARY)
            .map(|field| (field, exif))
    })
}

fn parse_exif_datetime(s: &str) -> Option<DateTime<Local>> {
    let candidates = ["%Y:%m:%d %H:%M:%S", "%Y-%m-%d %H:%M:%S"];
    for fmt in candidates {
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s.trim(), fmt) {
            if let Some(dt) = Local.from_local_datetime(&naive).earliest() {
                return Some(dt);
            }
            return Some(DateTime::from_naive_utc_and_offset(
                naive,
                *Local::now().offset(),
            ));
        }
    }
    None
}

fn format_flash_value(value: &Value) -> String {
    match value {
        Value::Short(values) if !values.is_empty() => match values[0] {
            0x00 => "not fired".to_string(),
            0x01 => "fired".to_string(),
            0x05 => "fired, no return light detection function".to_string(),
            0x07 => "fired, return light detection function".to_string(),
            0x09 => "fired, compulsory flash mode".to_string(),
            0x0D => "fired, compulsory flash mode, no return light detection function".to_string(),
            0x0F => "fired, compulsory flash mode, return light detection function".to_string(),
            0x10 => "not fired, compulsory flash suppression mode".to_string(),
            0x18 => "not fired, auto mode".to_string(),
            0x19 => "fired, auto mode".to_string(),
            0x1D => "fired, auto mode, no return light detection function".to_string(),
            0x1F => "fired, auto mode, return light detection function".to_string(),
            0x20 => "not fired, no flash function".to_string(),
            0x41 => "fired, red-eye reduction mode".to_string(),
            0x45 => "fired, red-eye reduction mode, no return light detection function".to_string(),
            0x47 => "fired, red-eye reduction mode, return light detection function".to_string(),
            0x49 => "fired, compulsory flash mode, red-eye reduction mode".to_string(),
            0x4D => {
                "fired, compulsory flash mode, red-eye reduction mode, no return light detection function"
                    .to_string()
            }
            0x4F => {
                "fired, compulsory flash mode, red-eye reduction mode, return light detection function"
                    .to_string()
            }
            0x59 => "fired, auto mode, red-eye reduction mode".to_string(),
            0x5D => {
                "fired, auto mode, red-eye reduction mode, no return light detection function"
                    .to_string()
            }
            0x5F => {
                "fired, auto mode, red-eye reduction mode, return light detection function"
                    .to_string()
            }
            _ => format!("unknown (0x{:02X})", values[0]),
        },
        _ => "unknown".to_string(),
    }
}

fn format_color_space(value: &Value) -> String {
    match value {
        Value::Short(values) if !values.is_empty() => match values[0] {
            1 => "sRGB".to_string(),
            65535 => "uncalibrated".to_string(),
            _ => format!("unknown ({})", values[0]),
        },
        _ => "unknown".to_string(),
    }
}

fn format_metering_mode(value: &Value) -> String {
    match value {
        Value::Short(values) if !values.is_empty() => match values[0] {
            0 => "unknown".to_string(),
            1 => "average".to_string(),
            2 => "center-weighted average".to_string(),
            3 => "spot".to_string(),
            4 => "multi-spot".to_string(),
            5 => "pattern".to_string(),
            6 => "partial".to_string(),
            255 => "other".to_string(),
            _ => format!("unknown ({})", values[0]),
        },
        _ => "unknown".to_string(),
    }
}

fn format_sensitivity_type(value: &Value) -> String {
    match value {
        Value::Short(values) if !values.is_empty() => match values[0] {
            0 => "unknown".to_string(),
            1 => "SOS".to_string(),
            2 => "REI".to_string(),
            3 => "ISO".to_string(),
            4 => "SOS and REI".to_string(),
            5 => "SOS and ISO".to_string(),
            6 => "REI and ISO".to_string(),
            7 => "SOS and REI and ISO".to_string(),
            _ => format!("unknown ({})", values[0]),
        },
        _ => "unknown".to_string(),
    }
}

fn format_exposure_program(value: &Value) -> String {
    match value {
        Value::Short(values) if !values.is_empty() => match values[0] {
            0 => "not defined".to_string(),
            1 => "manual".to_string(),
            2 => "normal program".to_string(),
            3 => "aperture priority".to_string(),
            4 => "shutter priority".to_string(),
            5 => "creative program (biased toward depth of field)".to_string(),
            6 => "action program (biased toward fast shutter speed)".to_string(),
            7 => "portrait mode (for closeup photos with the background out of focus)".to_string(),
            8 => "landscape mode (for landscape photos with the background in focus)".to_string(),
            _ => format!("unknown ({})", values[0]),
        },
        _ => "unknown".to_string(),
    }
}

fn format_orientation(value: &Value) -> String {
    match value {
        Value::Short(values) if !values.is_empty() => match values[0] {
            1 => "Normal (0°)".to_string(),
            2 => "Volteada horizontal".to_string(),
            3 => "Rotada 180°".to_string(),
            4 => "Volteada vertical".to_string(),
            5 => "Transpuesta".to_string(),
            6 => "Rotada 90° horario".to_string(),
            7 => "Transversa".to_string(),
            8 => "Rotada 90° antihorario".to_string(),
            _ => format!("Desconocida ({})", values[0]),
        },
        _ => "Desconocida".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sample(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("photos")
            .join(name)
    }

    #[test]
    fn extracts_cr3_sample_metadata() {
        let path = sample(
            "canon-eos-r50-2025-08-08-canon-eos-r50-2025-08-08-img_8333-56f54f9a-56f54f9a.cr3",
        );
        let metadata = extract_metadata(&path).unwrap();
        assert_ne!(metadata.image_info.dimensions, (0, 0));
        let exif = metadata.exif_info.expect("CR3 EXIF should be found");
        assert!(
            exif.camera_make
                .as_deref()
                .is_some_and(|m| m.contains("Canon")),
            "make={:?}",
            exif.camera_make
        );
        assert!(exif.orientation_code.is_some());
        assert!(exif.shutter_speed.is_some());
        assert!(exif.iso.is_some());
    }

    #[test]
    fn extracts_arw_sample_metadata() {
        let path = sample("_SON2447.ARW");
        let metadata = extract_metadata(&path).unwrap();
        assert_ne!(metadata.image_info.dimensions, (0, 0));
        let exif = metadata.exif_info.expect("ARW EXIF should be found");
        assert!(
            exif.camera_make
                .as_deref()
                .is_some_and(|m| m.contains("SONY")),
            "make={:?}",
            exif.camera_make
        );
        assert!(exif.orientation_code.is_some());
    }

    #[test]
    fn extracts_hif_sample_metadata() {
        let path = sample("IMG_8382.HIF");
        let metadata = extract_metadata(&path).unwrap();
        assert_ne!(metadata.image_info.dimensions, (0, 0));
    }

    #[test]
    fn corrupt_raw_reports_unknown_dimensions_instead_of_fake_6000x4000() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("img-meta-bad-{}-{nanos}.arw", std::process::id()));
        std::fs::write(&path, b"not a raw file at all").unwrap();
        let meta = extract_metadata(&path).unwrap();
        assert_eq!(meta.image_info.dimensions, (0, 0));
        assert_eq!(meta.image_info.aspect_ratio, "—");
        std::fs::remove_file(&path).unwrap();
    }
}
