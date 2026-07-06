use chrono::{DateTime, Local};
use std::path::Path;
use serde::{Deserialize, Serialize};
use std::io::BufReader;
use crate::errors::AppError;
use exif::Value;
use std::fs::File;
use memmap2::Mmap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageMetadata {
    pub file_info: FileInfo,
    pub image_info: ImageInfo,
    pub exif_info: Option<ExifInfo>,
}
// ... (struct definitions unchanged) ...
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

    // Optimization: Memory map large files to avoid reading them fully into heap memory
    // This is crucial for performance with large RAW files.
    let mmap = if file_info.size > 0 {
        if let Ok(file) = File::open(path) {
            unsafe { Mmap::map(&file).ok() }
        } else {
            None
        }
    } else {
        None
    };
    
    let file_data_slice = mmap.as_deref();

    // Image basic info without full decode
    let dimensions = if path.extension().is_some_and(|ext| {
        let ext = ext.to_string_lossy().to_lowercase();
        ext == "hif" || ext == "heic" || ext == "arw" || ext == "cr3"
    }) {
        // For HEIF and RAW files, we need special handling
        if path.extension().is_some_and(|ext| {
            let ext = ext.to_string_lossy().to_lowercase();
            ext == "hif" || ext == "heic"
        }) {
            use libheif_rs::HeifContext;
            
            // We need to ensure the data lives long enough for HeifContext
            let temp_data = if file_data_slice.is_none() {
                 std::fs::read(path).ok()
            } else {
                 None
            };
            
            let data_to_use = if let Some(d) = &temp_data {
                d.as_slice()
            } else if let Some(d) = file_data_slice {
                d
            } else {
                &[]
            };

            let ctx = if !data_to_use.is_empty() {
                 HeifContext::read_from_bytes(data_to_use).ok()
            } else {
                 None
            };

            if let Some(ctx) = ctx {
                 if let Ok(handle) = ctx.primary_image_handle() {
                     (handle.width(), handle.height())
                 } else {
                     (0, 0)
                 }
            } else {
                 (0, 0)
            }
        }
        // For ARW and CR3 files
        else if path.extension().is_some_and(|ext| {
            let ext = ext.to_string_lossy().to_lowercase();
            ext == "arw" || ext == "cr3"
        }) {
            if let Some(data) = file_data_slice {
                // Try quickraw first
                let jpg_data = match quickraw::Export::export_thumbnail_data(data) {
                    Ok((d, orientation)) => {
                        Some((d.to_vec(), orientation as u32))
                    },
                    Err(_) => {
                        // Fallback to manual extraction
                        extract_largest_jpeg(data).map(|d| (d, 1))
                    }
                };
                
                if let Some((jpg_data, orientation)) = jpg_data {
                    if let Ok(img) = image::load_from_memory_with_format(&jpg_data, image::ImageFormat::Jpeg) {
                         let (mut w, mut h) = (img.width(), img.height());
                          // Adjust for orientation
                          if matches!(orientation, 5..=8) {
                              std::mem::swap(&mut w, &mut h);
                          }
                         (w, h)
                    } else {
                        (6000, 4000)
                    }
                } else {
                     (6000, 4000)
                }
            } else {
                // If mapping failed, try old method (slow) or just return fallback
                // We fallback here to avoid erroring out completely
                (6000, 4000)
            }
        } else {
            image::image_dimensions(path).unwrap_or((0, 0))
        }
    } else {
        image::image_dimensions(path).unwrap_or((0, 0))
    };

    let megapixels = (dimensions.0 as f64 * dimensions.1 as f64) / 1_000_000.0;

    // Calculate aspect ratio
    let gcd_val = gcd(dimensions.0, dimensions.1);
    let ratio_w = if gcd_val > 0 { dimensions.0 / gcd_val } else { 0 };
    let ratio_h = if gcd_val > 0 { dimensions.1 / gcd_val } else { 0 };
    let aspect_ratio = format!("{}:{}", ratio_w, ratio_h);

    let image_info = ImageInfo {
        dimensions,
        megapixels,
        aspect_ratio,
        format: guess_format_from_path(path),
    };

    // Extract EXIF data based on file format, passing the mapped data
    let exif_info = extract_exif_data_by_format(path, file_data_slice);

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

fn extract_exif_data_by_format(path: &Path, data_slice: Option<&[u8]>) -> Option<ExifInfo> {
    let path_str = path.to_string_lossy().to_lowercase();

    if path_str.ends_with(".hif") || path_str.ends_with(".heic") {
        // Handle HEIF files using little_exif
        extract_heif_exif_data(path, data_slice)
    } else if path_str.ends_with(".arw") || path_str.ends_with(".cr3") {
        // Handle ARW and CR3 files using quickraw and TIFF format
        extract_raw_exif_data(path, data_slice)
    } else {
        // Use the original EXIF extraction for other formats
        extract_exif_data_from_file(path)
    }
}

fn extract_heif_exif_data(path: &Path, data_slice: Option<&[u8]>) -> Option<ExifInfo> {
    use libheif_rs::HeifContext;

    // Use mapped data or read from file
    let temp_data = if data_slice.is_none() {
         std::fs::read(path).ok()
    } else {
         None
    };
    
    let data_to_use = if let Some(d) = &temp_data {
        d.as_slice()
    } else if let Some(d) = data_slice {
        d
    } else {
        return None;
    };
    
    let ctx = HeifContext::read_from_bytes(data_to_use).ok()?;

    let handle = match ctx.primary_image_handle() {
        Ok(handle) => handle,
        Err(_) => return None,
    };

    // Get Exif metadata IDs
    let mut item_ids = [libheif_rs::ItemId::default(); 1]; // We only need the first one
    let count = handle.metadata_block_ids(&mut item_ids, "Exif".as_bytes());

    if count == 0 {
        return None;
    }

    // Use the metadata method to get the actual data
    let exif_data = match handle.metadata(item_ids[0]) {
        Ok(data) => data,
        Err(_) => return None,
    };

    // Extract EXIF bytes (skip the 4-byte TIFF header if present)
    let exif_bytes = if exif_data.len() > 4 {
        // The first 4 bytes may contain TIFF header offset
        &exif_data[4..]
    } else {
        &exif_data
    };

    // Directly try to parse the EXIF data
    let mut cursor = std::io::Cursor::new(exif_bytes);
    let exif_reader = exif::Reader::new();

    // Try parsing EXIF data directly
    match exif_reader.read_from_container(&mut cursor) {
        Ok(exif) => {
            convert_kamadak_exif_to_exif_info(&exif)
        },
        Err(_) => {
            // If direct parsing fails, try parsing as a raw EXIF structure
            // Check if it starts with EXIF marker
            let exif_data_without_header = if exif_bytes.len() > 6 && &exif_bytes[0..6] == b"Exif\0\0" {
                &exif_bytes[6..]  // Skip "Exif" + 2 null bytes
            } else {
                exif_bytes
            };

            let mut cursor = std::io::Cursor::new(exif_data_without_header);
            match exif_reader.read_from_container(&mut cursor) {
                Ok(exif) => {
                    convert_kamadak_exif_to_exif_info(&exif)
                },
                Err(_) => {
                    None
                },
            }
        },
    }
}

fn extract_raw_exif_data(path: &Path, data_slice: Option<&[u8]>) -> Option<ExifInfo> {
    // For RAW files, we should try to extract EXIF directly from the file
    // which is in TIFF format that can contain EXIF data

    // First, try to extract EXIF directly from the RAW file's TIFF structure
    match extract_tiff_exif_data(path) {
        Some(exif_info) => Some(exif_info),
        None => {
            // If that doesn't work, try to use the quickraw approach or manual
            if let Some(data) = data_slice {
                match quickraw::Export::export_thumbnail_data(data) {
                    Ok((jpg_data, _orientation)) => {
                        let file = std::io::Cursor::new(jpg_data);
                        let exif_reader = exif::Reader::new();
                        match exif_reader.read_from_container(&mut std::io::BufReader::new(file)) {
                            Ok(exif) => convert_kamadak_exif_to_exif_info(&exif),
                            Err(_) => None,
                        }
                    },
                    Err(_) => {
                        // Fallback
                        if let Some(jpg_data) = extract_largest_jpeg(data) {
                            let file = std::io::Cursor::new(jpg_data);
                            let exif_reader = exif::Reader::new();
                            match exif_reader.read_from_container(&mut std::io::BufReader::new(file)) {
                                Ok(exif) => convert_kamadak_exif_to_exif_info(&exif),
                                Err(_) => None,
                            }
                        } else {
                            None
                        }
                    }
                }
            } else {
                // If no data slice provided, read file (should be avoided now, but keep as fallback)
                let data = match std::fs::read(path) {
                    Ok(data) => data,
                    Err(_) => return None,
                };
                
                 match quickraw::Export::export_thumbnail_data(&data) {
                    Ok((jpg_data, _orientation)) => {
                        let file = std::io::Cursor::new(jpg_data);
                        let exif_reader = exif::Reader::new();
                        match exif_reader.read_from_container(&mut std::io::BufReader::new(file)) {
                            Ok(exif) => convert_kamadak_exif_to_exif_info(&exif),
                            Err(_) => None,
                        }
                    },
                    Err(_) => {
                        if let Some(jpg_data) = extract_largest_jpeg(&data) {
                            let file = std::io::Cursor::new(jpg_data);
                            let exif_reader = exif::Reader::new();
                            match exif_reader.read_from_container(&mut std::io::BufReader::new(file)) {
                                Ok(exif) => convert_kamadak_exif_to_exif_info(&exif),
                                Err(_) => None,
                            }
                        } else {
                            None
                        }
                    }
                }
            }
        }
    }
}

fn extract_largest_jpeg(data: &[u8]) -> Option<Vec<u8>> {
    let mut best_jpeg = None;
    let mut max_size = 0;
    
    let mut i = 0;
    
    while i < data.len().saturating_sub(2) {
        if data[i] == 0xFF && data[i+1] == 0xD8 {
            let start = i;
            let mut end = 0;
            let mut j = start + 2;
            while j < data.len().saturating_sub(1) {
                if data[j] == 0xFF && data[j+1] == 0xD9 {
                    end = j + 2;
                    let size = end - start;
                    if size > max_size {
                        max_size = size;
                        best_jpeg = Some((start, end));
                    }
                    i = end - 1;
                    break;
                }
                j += 1;
            }
            if end == 0 {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    
    best_jpeg.map(|(start, end)| data[start..end].to_vec())
}


fn extract_tiff_exif_data(path: &Path) -> Option<ExifInfo> {
    // ARW files are based on TIFF format, so we can try to read EXIF from the TIFF structure
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return None,
    };

    let mut buf_reader = std::io::BufReader::new(file);

    let exif_reader = exif::Reader::new();
    match exif_reader.read_from_container(&mut buf_reader) {
        Ok(exif) => {
            convert_kamadak_exif_to_exif_info(&exif)
        },
        Err(_) => {
            None
        },
    }
}

fn convert_kamadak_exif_to_exif_info(exif: &exif::Exif) -> Option<ExifInfo> {
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
    if let Some(field) = exif.get_field(exif::Tag::Make, exif::In::PRIMARY) {
        exif_info.camera_make = Some(field.display_value().with_unit(exif).to_string());
    }

    // Extract camera model
    if let Some(field) = exif.get_field(exif::Tag::Model, exif::In::PRIMARY) {
        exif_info.camera_model = Some(field.display_value().with_unit(exif).to_string());
    }

    // Extract date taken (DateTime Original)
    if let Some(field) = exif.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY) {
        let datetime_str = field.display_value().with_unit(exif).to_string();
        exif_info.date_taken = parse_exif_datetime(&datetime_str);
    }

    // Extract ISO (PhotographicSensitivity)
    if let Some(field) = exif.get_field(exif::Tag::PhotographicSensitivity, exif::In::PRIMARY) {
        if let Value::Short(values) = &field.value {
            if !values.is_empty() {
                exif_info.iso = Some(values[0] as u32);
            }
        }
    }

    // Extract focal length
    if let Some(field) = exif.get_field(exif::Tag::FocalLength, exif::In::PRIMARY) {
        exif_info.focal_length = Some(field.display_value().with_unit(exif).to_string());
    }

    // Extract aperture (FNumber)
    if let Some(field) = exif.get_field(exif::Tag::FNumber, exif::In::PRIMARY) {
        exif_info.aperture = Some(field.display_value().with_unit(exif).to_string());
    }

    // Extract shutter speed (ExposureTime)
    if let Some(field) = exif.get_field(exif::Tag::ExposureTime, exif::In::PRIMARY) {
        exif_info.shutter_speed = Some(field.display_value().with_unit(exif).to_string());
    }

    // Extract lens model
    if let Some(field) = exif.get_field(exif::Tag::LensModel, exif::In::PRIMARY) {
        exif_info.lens_model = Some(field.display_value().with_unit(exif).to_string());
    }

    // Extract flash information with descriptive text
    if let Some(field) = exif.get_field(exif::Tag::Flash, exif::In::PRIMARY) {
        exif_info.flash = Some(format_flash_value(&field.value));
    }

    // Extract ColorSpace
    if let Some(field) = exif.get_field(exif::Tag::ColorSpace, exif::In::PRIMARY) {
        exif_info.color_space = Some(format_color_space(&field.value));
    }

    // Extract MeteringMode
    if let Some(field) = exif.get_field(exif::Tag::MeteringMode, exif::In::PRIMARY) {
        exif_info.metering_mode = Some(format_metering_mode(&field.value));
    }

    // Extract ExposureBiasValue
    if let Some(field) = exif.get_field(exif::Tag::ExposureBiasValue, exif::In::PRIMARY) {
        exif_info.exposure_bias_value = Some(field.display_value().with_unit(exif).to_string());
    }

    // Extract ApertureValue
    if let Some(field) = exif.get_field(exif::Tag::ApertureValue, exif::In::PRIMARY) {
        exif_info.aperture_value = Some(field.display_value().with_unit(exif).to_string());
    }

    // Extract SensitivityType
    if let Some(field) = exif.get_field(exif::Tag::SensitivityType, exif::In::PRIMARY) {
        exif_info.sensitivity_type = Some(format_sensitivity_type(&field.value));
    }

    // Extract ExposureProgram
    if let Some(field) = exif.get_field(exif::Tag::ExposureProgram, exif::In::PRIMARY) {
        exif_info.exposure_program = Some(format_exposure_program(&field.value));
    }

    // Extract Orientation
    if let Some(field) = exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY) {
        exif_info.orientation = Some(format_orientation(&field.value));
    }

    // Extract GPS location (simplified)
    let lat = exif.get_field(exif::Tag::GPSLatitude, exif::In::PRIMARY);
    let lon = exif.get_field(exif::Tag::GPSLongitude, exif::In::PRIMARY);
    if let (Some(lat_field), Some(lon_field)) = (lat, lon) {
        let lat_str = lat_field.display_value().with_unit(exif).to_string();
        let lon_str = lon_field.display_value().with_unit(exif).to_string();
        exif_info.gps_location = Some(format!("{}, {}", lat_str, lon_str));
    }

    Some(exif_info)
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
            return Some(DateTime::from_naive_utc_and_offset(naive, *Local::now().offset()));
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
