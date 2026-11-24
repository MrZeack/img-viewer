//! Generación rápida de miniaturas por formato.
//!
//! - JPEG: escalado DCT (`jpeg-decoder`) decodifica directo a 1/2, 1/4 u 1/8.
//! - RAW (ARW/CR3): vista previa JPEG embebida vía mmap; solo se tocan sus páginas.
//! - HEIF: miniatura embebida de libheif cuando alcanza el tamaño pedido.
//! - Resto: decodificación completa con `image`.
//!
//! La orientación EXIF se aplica aquí; HEIF ya la aplica libheif.
use egui::ColorImage;
use image::{GrayImage, RgbImage};
use std::borrow::Cow;
use std::path::Path;

pub const JPEG_QUALITY: u8 = 82;
/// Los ARW guardan IFD0/EXIF al inicio; leer solo este prefijo evita copiar el RAW entero.
const EXIF_PREFIX_BYTES: usize = 512 * 1024;
/// Las vistas previas de CR3 están cerca del inicio del archivo.
const RAW_PREVIEW_SCAN_BYTES: usize = 16 * 1024 * 1024;
/// Una miniatura HEIF embebida se usa si su lado mayor alcanza este porcentaje del pedido.
const HEIF_THUMB_MIN_RATIO: f32 = 0.75;

/// Tamaño que cabe en un cuadrado `max`, conservando proporción y sin ampliar.
pub fn fit_size(w: u32, h: u32, max: u32) -> (u32, u32) {
    let (w, h, max) = (w.max(1), h.max(1), max.max(1));
    if w <= max && h <= max {
        return (w, h);
    }
    let scale = max as f64 / w.max(h) as f64;
    let fw = ((w as f64 * scale).round() as u32).clamp(1, max);
    let fh = ((h as f64 * scale).round() as u32).clamp(1, max);
    (fw, fh)
}

/// Lado mayor de la imagen de análisis (nitidez/exposición): la miniatura sola no
/// distingue un foco ligeramente errado.
pub const ANALYSIS_MAX: u32 = 800;

pub struct Generated {
    /// Miniatura orientada, lado mayor `max`.
    pub thumb: RgbImage,
    /// Escala de grises sin orientar, lado mayor ≤ `ANALYSIS_MAX`.
    pub analysis: GrayImage,
}

pub fn generate(path: &Path, max: u32) -> Result<Generated, String> {
    let (img, orientation) = decode_source(path, max.max(ANALYSIS_MAX))?;
    let mid = shrink_to(img, ANALYSIS_MAX);
    let analysis = image::imageops::grayscale(&mid);
    let thumb = shrink_to(mid, max);
    Ok(Generated {
        thumb: apply_exif_orientation(thumb, orientation.unwrap_or(1)),
        analysis,
    })
}

/// Solo la miniatura, decodificando al tamaño mínimo: para mostrar cuanto antes.
pub fn generate_thumb(path: &Path, max: u32) -> Result<RgbImage, String> {
    let (img, orientation) = decode_source(path, max)?;
    Ok(apply_exif_orientation(
        shrink_to(img, max),
        orientation.unwrap_or(1),
    ))
}

/// Decodifica a un tamaño ≥ `decode_max` (si el formato lo permite) y devuelve la orientación EXIF.
fn decode_source(path: &Path, decode_max: u32) -> Result<(RgbImage, Option<u32>), String> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    Ok(match ext.as_str() {
        "jpg" | "jpeg" => {
            let data = std::fs::read(path).map_err(|e| e.to_string())?;
            (
                decode_jpeg_scaled(&data, decode_max)?,
                container_orientation(&data),
            )
        }
        "arw" | "cr3" => {
            let map = map_file(path)?;
            let (preview, orientation) =
                raw_preview(&map).ok_or("RAW sin vista previa embebida")?;
            (decode_jpeg_scaled(&preview, decode_max)?, orientation)
        }
        "hif" | "heic" => {
            let map = map_file(path)?;
            (decode_heif(&map, decode_max)?, None)
        }
        _ => {
            let data = std::fs::read(path).map_err(|e| e.to_string())?;
            let img = image::load_from_memory(&data).map_err(|e| e.to_string())?;
            (img.to_rgb8(), container_orientation(&data))
        }
    })
}

fn shrink_to(img: RgbImage, max: u32) -> RgbImage {
    let (w, h) = fit_size(img.width(), img.height(), max);
    if (w, h) == img.dimensions() {
        img
    } else {
        image::imageops::thumbnail(&img, w, h)
    }
}

/// Vista previa JPEG embebida y orientación EXIF de un RAW.
/// CR3 se lee del contenedor ISO-BMFF; el resto (TIFF/ARW) con quickraw o escaneo acotado.
fn raw_preview(data: &[u8]) -> Option<(Cow<'_, [u8]>, Option<u32>)> {
    if crate::cr3::is_cr3(data) {
        if let Some(jpg) = crate::cr3::preview_jpeg(data) {
            return Some((Cow::Borrowed(jpg), crate::cr3::orientation(data)));
        }
    }
    let preview = crate::image_loader::safe_export_thumbnail(data)
        .map(|(jpg, _)| Cow::Owned(jpg))
        .or_else(|| bounded_preview_jpeg(data, RAW_PREVIEW_SCAN_BYTES).map(Cow::Borrowed))?;
    let orientation = tiff_orientation(data).or_else(|| container_orientation(&preview));
    Some((preview, orientation))
}

fn map_file(path: &Path) -> Result<memmap2::Mmap, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() == 0 {
        return Err("archivo vacío".into());
    }
    // SAFETY: solo lectura; si otro proceso trunca el archivo durante la lectura el
    // resultado es indefinido, el mismo riesgo que ya asume `metadata_simple`.
    unsafe { memmap2::Mmap::map(&file) }.map_err(|e| e.to_string())
}

pub fn decode_jpeg_scaled(data: &[u8], max: u32) -> Result<RgbImage, String> {
    use jpeg_decoder::{Decoder, PixelFormat};
    let mut decoder = Decoder::new(std::io::Cursor::new(data));
    decoder.read_info().map_err(|e| e.to_string())?;
    let info = decoder.info().ok_or("JPEG sin cabecera")?;
    let (tw, th) = fit_size(info.width as u32, info.height as u32, max);
    decoder
        .scale(
            tw.min(u16::MAX as u32) as u16,
            th.min(u16::MAX as u32) as u16,
        )
        .map_err(|e| e.to_string())?;
    let pixels = decoder.decode().map_err(|e| e.to_string())?;
    let info = decoder.info().ok_or("JPEG sin cabecera")?;
    let (w, h) = (info.width as u32, info.height as u32);
    let rgb = match info.pixel_format {
        PixelFormat::RGB24 => pixels,
        PixelFormat::L8 => pixels.iter().flat_map(|&l| [l, l, l]).collect(),
        PixelFormat::L16 => pixels
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|&[l, _]| [l, l, l])
            .collect(),
        PixelFormat::CMYK32 => pixels
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|&[c, m, y, k]| {
                let ch = |v: u8| ((v as u16 * k as u16) / 255) as u8;
                [ch(c), ch(m), ch(y)]
            })
            .collect(),
    };
    RgbImage::from_raw(w, h, rgb).ok_or_else(|| "buffer JPEG inválido".to_string())
}

/// Busca el final de un JPEG empezando en `start` (`FF D8`), respetando longitudes de segmentos
/// (como APP1/EXIF con miniatura embebida) para no truncar en un `FF D9` interno.
fn find_jpeg_end(data: &[u8], start: usize) -> Option<usize> {
    if data.get(start + 2) == Some(&0xFF) {
        if let Some(end) = find_jpeg_end_structured(data, start) {
            return Some(end);
        }
    }
    data[start + 2..]
        .windows(2)
        .position(|w| w == [0xFF, 0xD9])
        .map(|p| start + 2 + p + 2)
}

fn find_jpeg_end_structured(data: &[u8], start: usize) -> Option<usize> {
    let mut pos = start + 2;
    while pos < data.len() {
        if data[pos] != 0xFF {
            return None;
        }
        while pos < data.len() && data[pos] == 0xFF {
            pos += 1;
        }
        if pos >= data.len() {
            return None;
        }
        let marker = data[pos];
        pos += 1;

        match marker {
            0xD9 => return Some(pos),
            0x01 | 0xD0..=0xD8 => continue,
            0xDA => {
                // Start of Scan: skip header, then scan entropy-coded data to EOI.
                let len = u16::from_be_bytes([*data.get(pos)?, *data.get(pos + 1)?]) as usize;
                if len < 2 || pos + len > data.len() {
                    return None;
                }
                pos += len;
                while pos + 1 < data.len() {
                    if data[pos] == 0xFF {
                        let next = data[pos + 1];
                        if next == 0xD9 {
                            return Some(pos + 2);
                        } else if next == 0x00 || (0xD0..=0xD7).contains(&next) || next == 0xFF {
                            pos += 2;
                            continue;
                        }
                    }
                    pos += 1;
                }
                return None;
            }
            _ => {
                let len = u16::from_be_bytes([*data.get(pos)?, *data.get(pos + 1)?]) as usize;
                if len < 2 || pos + len > data.len() {
                    return None;
                }
                pos += len;
            }
        }
    }
    None
}

/// JPEG embebido más grande que empieza dentro de los primeros `limit` bytes.
pub fn bounded_preview_jpeg(data: &[u8], limit: usize) -> Option<&[u8]> {
    let scan_end = limit.min(data.len());
    let mut structured: Option<(usize, usize)> = None;
    let mut loose: Option<(usize, usize)> = None;
    let longer = |best: Option<(usize, usize)>, s: usize, e: usize| {
        if best.is_none_or(|(bs, be)| e - s > be - bs) {
            Some((s, e))
        } else {
            best
        }
    };
    let mut i = 0;
    while i + 1 < scan_end {
        if data[i] != 0xFF || data[i + 1] != 0xD8 {
            i += 1;
            continue;
        }
        let well_formed = (data.get(i + 2) == Some(&0xFF))
            .then(|| find_jpeg_end_structured(data, i))
            .flatten();
        if let Some(end) = well_formed {
            structured = longer(structured, i, end);
            i = end;
        } else if let Some(end) = find_jpeg_end(data, i) {
            // Sin estructura válida puede ser un `FF D8` suelto en datos del sensor: se recuerda
            // como último recurso sin saltar su tramo, que podría contener la vista previa real.
            loose = longer(loose, i, end);
            i += 2;
        } else {
            break;
        }
    }
    structured.or(loose).map(|(s, e)| &data[s..e])
}

fn orientation_of(exif: &exif::Exif) -> Option<u32> {
    exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)?
        .value
        .get_uint(0)
}

/// Orientación de JPEG/PNG/WebP (lee solo hasta el segmento EXIF).
fn container_orientation(data: &[u8]) -> Option<u32> {
    let exif = exif::Reader::new()
        .read_from_container(&mut std::io::Cursor::new(data))
        .ok()?;
    orientation_of(&exif)
}

/// Orientación de TIFF/ARW leyendo un prefijo; ignora IFDs que caen fuera.
fn tiff_orientation(data: &[u8]) -> Option<u32> {
    if !(data.starts_with(b"II*\0") || data.starts_with(b"MM\0*")) {
        return None;
    }
    let prefix = data[..data.len().min(EXIF_PREFIX_BYTES)].to_vec();
    let mut reader = exif::Reader::new();
    reader.continue_on_error(true);
    let exif = match reader.read_raw(prefix) {
        Ok(exif) => exif,
        Err(e) => e.distill_partial_result(|_| {}).ok()?,
    };
    orientation_of(&exif)
}

pub fn apply_exif_orientation(img: RgbImage, orientation: u32) -> RgbImage {
    use image::imageops::{flip_horizontal, flip_vertical, rotate180, rotate270, rotate90};
    match orientation {
        2 => flip_horizontal(&img),
        3 => rotate180(&img),
        4 => flip_vertical(&img),
        5 => flip_horizontal(&rotate90(&img)),
        6 => rotate90(&img),
        7 => flip_horizontal(&rotate270(&img)),
        8 => rotate270(&img),
        _ => img,
    }
}

fn decode_heif(data: &[u8], max: u32) -> Result<RgbImage, String> {
    use libheif_rs::{ColorSpace, HeifContext, ItemId, LibHeif, RgbChroma};
    let ctx = HeifContext::read_from_bytes(data).map_err(|e| e.to_string())?;
    let primary = ctx.primary_image_handle().map_err(|e| e.to_string())?;

    let mut ids: Vec<ItemId> = vec![0; primary.number_of_thumbnails()];
    let count = primary.thumbnail_ids(&mut ids);
    let min_edge = (max as f32 * HEIF_THUMB_MIN_RATIO) as u32;
    let thumb = ids[..count]
        .iter()
        .filter_map(|&id| primary.thumbnail(id).ok())
        .filter(|t| t.width().max(t.height()) >= min_edge)
        .min_by_key(|t| t.width().max(t.height()));
    let lib = LibHeif::new();
    let decode = |h| lib.decode(h, ColorSpace::Rgb(RgbChroma::Rgb), None);
    // Algunas cámaras escriben miniaturas que libheif rechaza: se cae a la imagen principal.
    let image = match thumb.as_ref().map(decode) {
        Some(Ok(image)) => image,
        _ => decode(&primary).map_err(|e| e.to_string())?,
    };
    let plane = image.planes().interleaved.ok_or("HEIF sin plano RGB")?;
    let (w, h) = (image.width(), image.height());
    let row = w as usize * 3;
    let mut buf = Vec::with_capacity(row * h as usize);
    for y in 0..h as usize {
        let start = y * plane.stride;
        buf.extend_from_slice(plane.data.get(start..start + row).ok_or("HEIF truncado")?);
    }
    RgbImage::from_raw(w, h, buf).ok_or_else(|| "buffer HEIF inválido".to_string())
}

pub fn encode_jpeg(img: &RgbImage) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity((img.width() * img.height() / 4) as usize);
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
        .encode_image(img)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

pub fn decode_thumb(bytes: &[u8]) -> Result<ColorImage, String> {
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg)
        .map_err(|e| e.to_string())?;
    Ok(to_color_image(&img.to_rgb8()))
}

pub fn to_color_image(img: &RgbImage) -> ColorImage {
    ColorImage::from_rgb([img.width() as usize, img.height() as usize], img.as_raw())
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

    fn gradient(w: u32, h: u32) -> RgbImage {
        RgbImage::from_fn(w, h, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
        })
    }

    fn long_edge(img: &RgbImage) -> u32 {
        img.width().max(img.height())
    }

    #[test]
    fn fit_size_keeps_aspect_and_never_upscales() {
        assert_eq!(fit_size(6000, 4000, 320), (320, 213));
        assert_eq!(fit_size(4000, 6000, 320), (213, 320));
        assert_eq!(fit_size(100, 50, 320), (100, 50));
        assert_eq!(fit_size(10000, 1, 320), (320, 1));
        assert_eq!(fit_size(0, 0, 320), (1, 1));
    }

    #[test]
    fn jpeg_is_decoded_scaled_then_fitted() {
        let jpeg = encode_jpeg(&gradient(2400, 1600)).unwrap();
        let scaled = decode_jpeg_scaled(&jpeg, 320).unwrap();
        assert_eq!((scaled.width(), scaled.height()), (600, 400), "DCT 1/4");

        let dir = std::env::temp_dir().join(format!("ivt-gen-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("big.JPG");
        std::fs::write(&path, &jpeg).unwrap();
        let generated = generate(&path, 320).unwrap();
        assert_eq!(generated.thumb.dimensions(), (320, 213));
        assert_eq!(generate_thumb(&path, 320).unwrap().dimensions(), (320, 213));
        assert_eq!(
            generated.analysis.dimensions(),
            (ANALYSIS_MAX, 533),
            "análisis a 800 px"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn bounded_preview_picks_largest_jpeg_within_limit() {
        let mut data = vec![0u8; 50];
        let small_start = data.len();
        data.extend([0xFF, 0xD8]);
        data.extend(vec![1u8; 100]);
        data.extend([0xFF, 0xD9]);
        data.extend(vec![0u8; 20]);
        let big_start = data.len();
        data.extend([0xFF, 0xD8]);
        data.extend(vec![2u8; 500]);
        data.extend([0xFF, 0xD9]);

        assert_eq!(bounded_preview_jpeg(&data, data.len()).unwrap().len(), 504);
        assert_eq!(bounded_preview_jpeg(&data, big_start).unwrap().len(), 104);
        assert_eq!(bounded_preview_jpeg(&data[..small_start], 1000), None);
    }

    #[test]
    fn bounded_preview_prefers_well_formed_jpeg_over_stray_markers() {
        // ARW reescrito (p. ej. por exiftool): la vista previa queda tras los datos del sensor,
        // donde aparecen `FF D8` sueltos sin estructura JPEG.
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xDA, 0x00, 0x04, 0x01, 0x02];
        jpeg.extend(vec![0xAA; 64]);
        jpeg.extend([0xFF, 0xD9]);
        let mut blob = vec![0x11; 16];
        blob.extend([0xFF, 0xD8, 0x00, 0x13]); // marcador suelto en datos del sensor
        blob.extend(vec![0x22; 500]);
        let start = blob.len();
        blob.extend(&jpeg);
        blob.extend(vec![0x33; 16]);
        let found = bounded_preview_jpeg(&blob, blob.len()).unwrap();
        assert_eq!(found, &blob[start..start + jpeg.len()]);
    }

    #[test]
    fn bounded_preview_skips_embedded_exif_thumbnail_eoi() {
        // Outer JPEG with an APP1 segment containing an inner FF D8 ... FF D9 thumbnail.
        let mut jpeg = vec![0xFF, 0xD8];
        let mut app1_payload = b"Exif\0\0".to_vec();
        app1_payload.extend([0xFF, 0xD8, 0x11, 0x22, 0xFF, 0xD9]); // embedded thumb
        let app1_len = (app1_payload.len() + 2) as u16;
        jpeg.extend([0xFF, 0xE1]);
        jpeg.extend(app1_len.to_be_bytes());
        jpeg.extend(app1_payload);
        // SOS header (len 4) + entropy data + outer EOI
        jpeg.extend([0xFF, 0xDA, 0x00, 0x04, 0x01, 0x02]);
        jpeg.extend(vec![0xAA; 200]);
        jpeg.extend([0xFF, 0xD9]);

        let mut blob = vec![0u8; 32];
        blob.extend(&jpeg);
        let found = bounded_preview_jpeg(&blob, blob.len()).unwrap();
        assert_eq!(
            found.len(),
            jpeg.len(),
            "must not truncate outer JPEG at APP1 embedded thumbnail EOI"
        );
    }

    #[test]
    fn exif_orientation_rotates_and_flips() {
        let img = gradient(30, 20);
        let corner = *img.get_pixel(29, 0);
        let rotated = apply_exif_orientation(img.clone(), 6);
        assert_eq!((rotated.width(), rotated.height()), (20, 30));
        assert_eq!(
            *rotated.get_pixel(19, 29),
            corner,
            "90° CW lleva (w-1,0) a (h-1,w-1)"
        );
        assert_eq!(apply_exif_orientation(img.clone(), 1), img);
        assert_eq!(apply_exif_orientation(img.clone(), 99), img);
    }

    #[test]
    fn encoded_thumb_roundtrips_to_color_image() {
        let jpeg = encode_jpeg(&gradient(320, 213)).unwrap();
        assert_eq!(decode_thumb(&jpeg).unwrap().size, [320, 213]);
        assert!(decode_thumb(b"not a jpeg").is_err());
    }

    #[test]
    fn real_raw_and_heif_samples_produce_bounded_thumbnails() {
        for name in [
            "_SON2447.ARW",
            "canon-eos-r50-2025-08-08-canon-eos-r50-2025-08-08-img_8333-56f54f9a-56f54f9a.cr3",
            "IMG_8382.HIF",
        ] {
            let generated = generate(&sample(name), 320).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(long_edge(&generated.thumb), 320, "{name}");
            let (w, h) = generated.analysis.dimensions();
            assert_eq!(w.max(h), ANALYSIS_MAX, "{name}");
        }
    }

    #[test]
    fn cr3_uses_container_preview_and_orientation() {
        let path = sample(
            "canon-eos-r50-2025-08-08-canon-eos-r50-2025-08-08-img_8333-56f54f9a-56f54f9a.cr3",
        );
        let data = std::fs::read(&path).unwrap();
        let (preview, orientation) = raw_preview(&data).unwrap();
        assert_eq!(&*preview, crate::cr3::preview_jpeg(&data).unwrap());
        assert_eq!(orientation, crate::cr3::orientation(&data));
    }

    #[test]
    fn unreadable_files_return_error_without_panicking() {
        let dir = std::env::temp_dir().join(format!("ivt-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in [
            "empty.jpg",
            "empty.arw",
            "empty.cr3",
            "empty.hif",
            "empty.png",
        ] {
            std::fs::write(dir.join(name), b"").unwrap();
            assert!(generate(&dir.join(name), 320).is_err(), "{name}");
        }
        assert!(generate(&dir.join("missing.jpg"), 320).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// `IMG_BENCH_DIR=<carpeta> cargo test --release bench_generate -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn bench_generate() {
        let dir = std::env::var("IMG_BENCH_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| sample(""));
        let files = crate::image_loader::get_image_files(&dir, false).unwrap();
        let mut by_ext: std::collections::BTreeMap<String, (u32, std::time::Duration)> =
            Default::default();
        let total = std::time::Instant::now();
        for f in &files {
            let ext = f.extension().unwrap().to_string_lossy().to_lowercase();
            let t = std::time::Instant::now();
            let ok = generate(f, 384).and_then(|g| encode_jpeg(&g.thumb)).is_ok();
            let e = by_ext.entry(ext).or_default();
            e.0 += ok as u32;
            e.1 += t.elapsed();
        }
        for (ext, (n, d)) in by_ext {
            println!(
                "{ext}: {n} ok, {:.1} ms/img",
                d.as_secs_f64() * 1000.0 / n.max(1) as f64
            );
        }
        println!(
            "total {} files in {:.2?} (1 hilo)",
            files.len(),
            total.elapsed()
        );
    }
}
