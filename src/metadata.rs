use anyhow::Result;
use chrono::{DateTime, Local};
use kamadak_exif::{Reader, In, Tag, Value};
use std::path::Path;
use serde::{Deserialize, Serialize};

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
}

pub async fn extract_metadata(path: &Path) -> Result<ImageMetadata> {
    let metadata = tokio::fs::metadata(path).await?;
    
    // File info
    let file_info = FileInfo {
        name: path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        size: metadata.len(),
        created: metadata.created().ok()
            .and_then(|t| DateTime::from(t).with_timezone(&Local).into()),
        modified: metadata.modified().ok()
            .and_then(|t| DateTime::from(t).with_timezone(&Local).into()),
        path: path.parent()
            .unwrap_or(Path::new(""))
            .to_string_lossy()
            .to_string(),
    };
    
    // Load image for basic info
    let img_data = tokio::fs::read(path).await?;
    let img = image::load_from_memory(&img_data)?;
    
    let dimensions = (img.width(), img.height());
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
    
    // Extract EXIF data
    let exif_info = extract_exif_data(&img_data).ok();
    
    Ok(ImageMetadata {
        file_info,
        image_info,
        exif_info,
    })
}

fn extract_exif_data(data: &[u8]) -> Result<ExifInfo> {
    let mut cursor = std::io::Cursor::new(data);
    let exifreader = Reader::new();
    let exif = exifreader.read_from_container(&mut cursor)?;
    
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
    };
    
    // Extract camera make and model
    if let Some(field) = exif.get_field(Tag::Make, In::PRIMARY) {
        exif_info.camera_make = Some(field.display_value().to_string());
    }
    
    if let Some(field) = exif.get_field(Tag::Model, In::PRIMARY) {
        exif_info.camera_model = Some(field.display_value().to_string());
    }
    
    // Extract date taken
    if let Some(field) = exif.get_field(Tag::DateTimeOriginal, In::PRIMARY) {
        if let Ok(datetime_str) = field.display_value().to_string().parse::<String>() {
            if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&datetime_str, "%Y-%m-%d %H:%M:%S") {
                exif_info.date_taken = Some(Local.from_local_datetime(&dt).unwrap());
            }
        }
    }
    
    // Extract exposure settings
    if let Some(field) = exif.get_field(Tag::ExposureTime, In::PRIMARY) {
        exif_info.shutter_speed = Some(format_shutter_speed(&field.value));
    }
    
    if let Some(field) = exif.get_field(Tag::FNumber, In::PRIMARY) {
        exif_info.aperture = Some(format_aperture(&field.value));
    }
    
    if let Some(field) = exif.get_field(Tag::PhotographicSensitivity, In::PRIMARY) {
        if let Value::Short(ref values) = field.value {
            if !values.is_empty() {
                exif_info.iso = Some(values[0] as u32);
            }
        }
    }
    
    if let Some(field) = exif.get_field(Tag::FocalLength, In::PRIMARY) {
        exif_info.focal_length = Some(format_focal_length(&field.value));
    }
    
    if let Some(field) = exif.get_field(Tag::Flash, In::PRIMARY) {
        exif_info.flash = Some(format_flash(&field.value));
    }
    
    if let Some(field) = exif.get_field(Tag::LensModel, In::PRIMARY) {
        exif_info.lens_model = Some(field.display_value().to_string());
    }
    
    // Extract GPS if available
    if let (Some(lat_field), Some(lon_field)) = (
        exif.get_field(Tag::GPSLatitude, In::PRIMARY),
        exif.get_field(Tag::GPSLongitude, In::PRIMARY),
    ) {
        if let (Some(lat_ref), Some(lon_ref)) = (
            exif.get_field(Tag::GPSLatitudeRef, In::PRIMARY),
            exif.get_field(Tag::GPSLongitudeRef, In::PRIMARY),
        ) {
            let lat = parse_gps_coordinate(&lat_field.value, &lat_ref.value);
            let lon = parse_gps_coordinate(&lon_field.value, &lon_ref.value);
            exif_info.gps_location = Some(format!("{:.6}°, {:.6}°", lat, lon));
        }
    }
    
    Ok(exif_info)
}

fn format_shutter_speed(value: &Value) -> String {
    match value {
        Value::Rational(ref rationals) if !rationals.is_empty() => {
            let rational = &rationals[0];
            if rational.denom == 1 {
                format!("{}s", rational.num)
            } else {
                format!("1/{}s", rational.denom / rational.num)
            }
        }
        _ => "Unknown".to_string(),
    }
}

fn format_aperture(value: &Value) -> String {
    match value {
        Value::Rational(ref rationals) if !rationals.is_empty() => {
            let rational = &rationals[0];
            let f_stop = rational.num as f64 / rational.denom as f64;
            format!("f/{:.1}", f_stop)
        }
        _ => "Unknown".to_string(),
    }
}

fn format_focal_length(value: &Value) -> String {
    match value {
        Value::Rational(ref rationals) if !rationals.is_empty() => {
            let rational = &rationals[0];
            let focal = rational.num as f64 / rational.denom as f64;
            format!("{:.0}mm", focal)
        }
        _ => "Unknown".to_string(),
    }
}

fn format_flash(value: &Value) -> String {
    match value {
        Value::Short(ref values) if !values.is_empty() => {
            match values[0] {
                0 => "No Flash".to_string(),
                1 => "Flash Fired".to_string(),
                5 => "Flash Fired, No Return".to_string(),
                7 => "Flash Fired, Return".to_string(),
                16 => "No Flash, Compulsory".to_string(),
                24 => "No Flash, Auto".to_string(),
                25 => "Flash Fired, Auto".to_string(),
                31 => "Flash Fired, Auto, Return".to_string(),
                32 => "No Flash Available".to_string(),
                _ => format!("Unknown ({})", values[0]),
            }
        }
        _ => "Unknown".to_string(),
    }
}

fn parse_gps_coordinate(coord_value: &Value, ref_value: &Value) -> f64 {
    if let (Value::Rational(coords), Value::Ascii(ref_bytes)) = (coord_value, ref_value) {
        if coords.len() >= 3 {
            let degrees = coords[0].num as f64 / coords[0].denom as f64;
            let minutes = coords[1].num as f64 / coords[1].denom as f64;
            let seconds = coords[2].num as f64 / coords[2].denom as f64;
            
            let mut decimal = degrees + minutes / 60.0 + seconds / 3600.0;
            
            if let Ok(ref_str) = std::str::from_utf8(&ref_bytes[0]) {
                if ref_str == "S" || ref_str == "W" {
                    decimal = -decimal;
                }
            }
            
            return decimal;
        }
    }
    0.0
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