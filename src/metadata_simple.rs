use chrono::{DateTime, Local};
use std::path::Path;
use serde::{Deserialize, Serialize};
use exif::Value;
use std::io::BufReader;
use crate::errors::AppError;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

pub fn extract_metadata(path: &Path) -> Result<ImageMetadata, AppError> {
    let metadata = std::fs::metadata(path)
        .map_err(|e| AppError::MetadataError(format!("Failed to get file metadata for {}: {}", path.display(), e)))?;
    
    // File info
    let file_info = FileInfo {
        name: path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        size: metadata.len(),
        created: metadata.created().ok()
            .and_then(|t| {
                use std::time::SystemTime;
                let duration = t.duration_since(SystemTime::UNIX_EPOCH).ok()?;
                DateTime::from_timestamp(duration.as_secs() as i64, duration.subsec_nanos()).map(|dt| dt.with_timezone(&Local))
            }),
        modified: metadata.modified().ok()
            .and_then(|t| {
                use std::time::SystemTime;
                let duration = t.duration_since(SystemTime::UNIX_EPOCH).ok()?;
                DateTime::from_timestamp(duration.as_secs() as i64, duration.subsec_nanos()).map(|dt| dt.with_timezone(&Local))
            }),
        path: path.parent()
            .unwrap_or(Path::new(""))
            .to_string_lossy()
            .to_string(),
    };
    
    // Image basic info without full decode
    let (w, h) = image::image_dimensions(path)
        .map_err(|e| AppError::MetadataError(format!("Failed to get image dimensions for {}: {}", path.display(), e)))?;
    let dimensions = (w, h);
    let megapixels = (dimensions.0 as f64 * dimensions.1 as f64) / 1_000_000.0;
    
    // Calculate aspect ratio
    let gcd = gcd(dimensions.0, dimensions.1);
    let ratio_w = dimensions.0 / gcd;
    let ratio_h = dimensions.1 / gcd;
    let aspect_ratio = format!("{}:{}", ratio_w, ratio_h);
    
    let image_info = ImageInfo {
        dimensions,
        megapixels,
        aspect_ratio,
        format: guess_format_from_path(path),
    };
    
    // Extract EXIF data directly from file
    let exif_info = extract_exif_data_from_file(path);
    
    Ok(ImageMetadata {
        file_info,
        image_info,
        exif_info,
    })
}

fn guess_format_from_path(path: &Path) -> String {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_uppercase())
        .unwrap_or_else(|| "Unknown".to_string())
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
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

fn extract_exif_data_from_file(path: &Path) -> Option<ExifInfo> {
    use exif::{Reader, Tag, In, Value};

    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return None, // Return None if file can't be opened
    };
    let mut buf_reader = BufReader::new(file);
    let exif_reader = Reader::new();

    match exif_reader.read_from_container(&mut buf_reader) {
        Ok(exif) => {
            let mut exif_info = ExifInfo {
                camera_make: None,
                camera_model: None,
                date_taken: None,
                shutter_speed: None,
                aperture: None,
                iso: None,
                focal_length: None,
                flash: None,
                lens_model: None,
                gps_location: None,
                color_space: None,
                metering_mode: None,
                exposure_bias_value: None,
                aperture_value: None,
                sensitivity_type: None,
                exposure_program: None,
                orientation: None,
            };

            // Extract camera make
            if let Some(field) = exif.get_field(Tag::Make, In::PRIMARY) {
                exif_info.camera_make = Some(field.display_value().with_unit(&exif).to_string());
            }

            // Extract camera model
            if let Some(field) = exif.get_field(Tag::Model, In::PRIMARY) {
                exif_info.camera_model = Some(field.display_value().with_unit(&exif).to_string());
            }

            // Extract date taken (DateTime Original)
            if let Some(field) = exif.get_field(Tag::DateTimeOriginal, In::PRIMARY) {
                let datetime_str = field.display_value().with_unit(&exif).to_string();
                exif_info.date_taken = parse_exif_datetime(&datetime_str);
            }

            // Extract ISO (PhotographicSensitivity)
            if let Some(field) = exif.get_field(Tag::PhotographicSensitivity, In::PRIMARY) {
                if let Value::Short(values) = &field.value {
                    if !values.is_empty() {
                        exif_info.iso = Some(values[0] as u32);
                    }
                }
            }

            // Extract focal length
            if let Some(field) = exif.get_field(Tag::FocalLength, In::PRIMARY) {
                exif_info.focal_length = Some(field.display_value().with_unit(&exif).to_string());
            }

            // Extract aperture (FNumber)
            if let Some(field) = exif.get_field(Tag::FNumber, In::PRIMARY) {
                exif_info.aperture = Some(field.display_value().with_unit(&exif).to_string());
            }

            // Extract shutter speed (ExposureTime)
            if let Some(field) = exif.get_field(Tag::ExposureTime, In::PRIMARY) {
                exif_info.shutter_speed = Some(field.display_value().with_unit(&exif).to_string());
            }

            // Extract lens model
            if let Some(field) = exif.get_field(Tag::LensModel, In::PRIMARY) {
                exif_info.lens_model = Some(field.display_value().with_unit(&exif).to_string());
            }

            // Extract flash information with descriptive text
            if let Some(field) = exif.get_field(Tag::Flash, In::PRIMARY) {
                exif_info.flash = Some(format_flash_value(&field.value));
            }

            // Extract ColorSpace
            if let Some(field) = exif.get_field(Tag::ColorSpace, In::PRIMARY) {
                exif_info.color_space = Some(format_color_space(&field.value));
            }

            // Extract MeteringMode
            if let Some(field) = exif.get_field(Tag::MeteringMode, In::PRIMARY) {
                exif_info.metering_mode = Some(format_metering_mode(&field.value));
            }

            // Extract ExposureBiasValue
            if let Some(field) = exif.get_field(Tag::ExposureBiasValue, In::PRIMARY) {
                exif_info.exposure_bias_value = Some(field.display_value().with_unit(&exif).to_string());
            }

            // Extract ApertureValue
            if let Some(field) = exif.get_field(Tag::ApertureValue, In::PRIMARY) {
                exif_info.aperture_value = Some(field.display_value().with_unit(&exif).to_string());
            }

            // Extract SensitivityType
            if let Some(field) = exif.get_field(Tag::SensitivityType, In::PRIMARY) {
                exif_info.sensitivity_type = Some(format_sensitivity_type(&field.value));
            }

            // Extract ExposureProgram
            if let Some(field) = exif.get_field(Tag::ExposureProgram, In::PRIMARY) {
                exif_info.exposure_program = Some(format_exposure_program(&field.value));
            }

            // Extract Orientation
            if let Some(field) = exif.get_field(Tag::Orientation, In::PRIMARY) {
                exif_info.orientation = Some(format_orientation(&field.value));
            }

            // Extract GPS location (simplified)
            let lat = exif.get_field(Tag::GPSLatitude, In::PRIMARY);
            let lon = exif.get_field(Tag::GPSLongitude, In::PRIMARY);
            if let (Some(lat_field), Some(lon_field)) = (lat, lon) {
                let lat_str = lat_field.display_value().with_unit(&exif).to_string();
                let lon_str = lon_field.display_value().with_unit(&exif).to_string();
                exif_info.gps_location = Some(format!("{}, {}", lat_str, lon_str));
            }

            Some(exif_info)
        }
        Err(_) => {
            // If EXIF parsing fails, return None (no EXIF data available)
            None
        }
    }
}

fn parse_exif_datetime(s: &str) -> Option<DateTime<Local>> {
    // Common EXIF patterns: "YYYY:MM:DD HH:MM:SS" and "YYYY-MM-DD HH:MM:SS"
    let candidates = ["%Y:%m:%d %H:%M:%S", "%Y-%m-%d %H:%M:%S"];
    for fmt in candidates.iter() {
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, fmt) {
            return Some(DateTime::from_naive_utc_and_offset(naive, Local::now().offset().clone()));
        }
    }
    None
}

fn format_flash_value(value: &Value) -> String {
    match value {
        Value::Short(values) if !values.is_empty() => {
            match values[0] {
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
                0x4D => "fired, compulsory flash mode, red-eye reduction mode, no return light detection function".to_string(),
                0x4F => "fired, compulsory flash mode, red-eye reduction mode, return light detection function".to_string(),
                0x59 => "fired, auto mode, red-eye reduction mode".to_string(),
                0x5D => "fired, auto mode, red-eye reduction mode, no return light detection function".to_string(),
                0x5F => "fired, auto mode, red-eye reduction mode, return light detection function".to_string(),
                _ => format!("unknown (0x{:02X})", values[0]),
            }
        }
        _ => "unknown".to_string(),
    }
}

fn format_color_space(value: &Value) -> String {
    match value {
        Value::Short(values) if !values.is_empty() => {
            match values[0] {
                1 => "sRGB".to_string(),
                65535 => "uncalibrated".to_string(),
                _ => format!("unknown ({})", values[0]),
            }
        }
        _ => "unknown".to_string(),
    }
}

fn format_metering_mode(value: &Value) -> String {
    match value {
        Value::Short(values) if !values.is_empty() => {
            match values[0] {
                0 => "unknown".to_string(),
                1 => "average".to_string(),
                2 => "center-weighted average".to_string(),
                3 => "spot".to_string(),
                4 => "multi-spot".to_string(),
                5 => "pattern".to_string(),
                6 => "partial".to_string(),
                255 => "other".to_string(),
                _ => format!("unknown ({})", values[0]),
            }
        }
        _ => "unknown".to_string(),
    }
}

fn format_sensitivity_type(value: &Value) -> String {
    match value {
        Value::Short(values) if !values.is_empty() => {
            match values[0] {
                0 => "unknown".to_string(),
                1 => "SOS".to_string(),
                2 => "REI".to_string(),
                3 => "ISO".to_string(),
                4 => "SOS and REI".to_string(),
                5 => "SOS and ISO".to_string(),
                6 => "REI and ISO".to_string(),
                7 => "SOS and REI and ISO".to_string(),
                _ => format!("unknown ({})", values[0]),
            }
        }
        _ => "unknown".to_string(),
    }
}

fn format_exposure_program(value: &Value) -> String {
    match value {
        Value::Short(values) if !values.is_empty() => {
            match values[0] {
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
            }
        }
        _ => "unknown".to_string(),
    }
}

fn format_orientation(value: &Value) -> String {
    match value {
        Value::Short(values) if !values.is_empty() => {
            match values[0] {
                1 => "row 0 at top and column 0 at left".to_string(),
                2 => "row 0 at top and column 0 at right".to_string(),
                3 => "row 0 at bottom and column 0 at right".to_string(),
                4 => "row 0 at bottom and column 0 at left".to_string(),
                5 => "row 0 at left and column 0 at top".to_string(),
                6 => "row 0 at right and column 0 at top".to_string(),
                7 => "row 0 at right and column 0 at bottom".to_string(),
                8 => "row 0 at left and column 0 at bottom".to_string(),
                _ => format!("unknown ({})", values[0]),
            }
        }
        _ => "unknown".to_string(),
    }
}
