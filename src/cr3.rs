//! Minimal reader for Canon CR3 files.
//!
//! CR3 is an ISO-BMFF container (like MP4/HEIF), which neither kamadak-exif nor
//! quickraw understand. The EXIF data is split across standalone TIFF blocks
//! inside the Canon `uuid` box under `moov`:
//!
//! - `CMT1`: IFD0 (Make, Model, Orientation, ...)
//! - `CMT2`: Exif IFD (exposure, aperture, ISO, lens, ...)
//! - `CMT3`: Canon maker notes (unused)
//! - `CMT4`: GPS IFD
//!
//! The full-size JPEG preview is the single sample of the first `trak`.

const CANON_UUID: [u8; 16] = [
    0x85, 0xc0, 0xb6, 0x87, 0x82, 0x0f, 0x11, 0xe0, 0x81, 0x11, 0xf4, 0xce, 0x46, 0x2b, 0x6a, 0x48,
];

struct BmffBox<'a> {
    kind: [u8; 4],
    uuid: &'a [u8],
    body: &'a [u8],
}

/// Iterates over the boxes laid out back to back in `data`, stopping at the
/// first malformed one.
fn boxes(data: &[u8]) -> impl Iterator<Item = BmffBox<'_>> {
    let mut rest = data;
    std::iter::from_fn(move || {
        let size32 = be_u32(rest, 0)?;
        let kind: [u8; 4] = rest.get(4..8)?.try_into().ok()?;
        let (size, mut header) = match size32 {
            0 => (rest.len() as u64, 8),
            1 => (be_u64(rest, 8)?, 16),
            s => (s as u64, 8),
        };
        let uuid_start = header;
        if kind == *b"uuid" {
            header += 16;
        }
        if size < header as u64 || size > rest.len() as u64 {
            rest = &[];
            return None;
        }
        let size = size as usize;
        let parsed = BmffBox {
            kind,
            uuid: if kind == *b"uuid" {
                &rest[uuid_start..header]
            } else {
                &[]
            },
            body: &rest[header..size],
        };
        rest = &rest[size..];
        Some(parsed)
    })
}

fn find<'a>(data: &'a [u8], kind: &[u8; 4]) -> Option<BmffBox<'a>> {
    boxes(data).find(|b| b.kind == *kind)
}

fn be_u32(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        data.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn be_u64(data: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_be_bytes(
        data.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

/// Returns true when `data` starts with an `ftyp` box of major brand `crx `.
pub fn is_cr3(data: &[u8]) -> bool {
    boxes(data)
        .next()
        .is_some_and(|b| b.kind == *b"ftyp" && b.body.starts_with(b"crx "))
}

fn canon_box(data: &[u8]) -> Option<&[u8]> {
    let moov = find(data, b"moov")?;
    boxes(moov.body)
        .find(|b| b.kind == *b"uuid" && b.uuid == CANON_UUID)
        .map(|b| b.body)
}

/// CMT2/CMT4 hold a bare sub-IFD stored where IFD0 would be, so kamadak-exif
/// would tag their fields with the IFD0 context (e.g. `ExposureTime` would not
/// match `Tag::ExposureTime`). Append a one-entry IFD0 pointing at the original
/// IFD through `pointer_tag` so it is parsed in the right context. The original
/// bytes stay in place, so every internal offset remains valid.
fn wrap_sub_ifd(block: &[u8], pointer_tag: exif::Tag) -> Option<Vec<u8>> {
    let little_endian = match block.get(0..2)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let u16_bytes = |v: u16| {
        if little_endian {
            v.to_le_bytes()
        } else {
            v.to_be_bytes()
        }
    };
    let u32_bytes = |v: u32| {
        if little_endian {
            v.to_le_bytes()
        } else {
            v.to_be_bytes()
        }
    };

    let sub_ifd_offset: [u8; 4] = block.get(4..8)?.try_into().ok()?;
    let mut tiff = block.to_vec();
    if tiff.len() % 2 == 1 {
        tiff.push(0);
    }
    let ifd0_offset = u32::try_from(tiff.len()).ok()?;
    tiff.extend_from_slice(&u16_bytes(1)); // entry count
    tiff.extend_from_slice(&u16_bytes(pointer_tag.number()));
    tiff.extend_from_slice(&u16_bytes(4)); // LONG
    tiff.extend_from_slice(&u32_bytes(1)); // count
    tiff.extend_from_slice(&sub_ifd_offset);
    tiff.extend_from_slice(&u32_bytes(0)); // no next IFD
    tiff[4..8].copy_from_slice(&u32_bytes(ifd0_offset));
    Some(tiff)
}

/// Parses the CMT1 (IFD0), CMT2 (Exif) and CMT4 (GPS) blocks. Fields must be
/// looked up across all returned `Exif`s.
pub fn read_exif(data: &[u8]) -> Vec<exif::Exif> {
    let Some(canon) = canon_box(data) else {
        return Vec::new();
    };
    let reader = exif::Reader::new();
    [
        (b"CMT1", None),
        (b"CMT2", Some(exif::Tag::ExifIFDPointer)),
        (b"CMT4", Some(exif::Tag::GPSInfoIFDPointer)),
    ]
    .iter()
    .filter_map(|(kind, pointer_tag)| {
        let block = find(canon, kind)?.body;
        let tiff = match pointer_tag {
            Some(tag) => wrap_sub_ifd(block, *tag)?,
            None => block.to_vec(),
        };
        reader.read_raw(tiff).ok()
    })
    .collect()
}

/// EXIF orientation (1-8) from CMT1.
pub fn orientation(data: &[u8]) -> Option<u32> {
    let block = find(canon_box(data)?, b"CMT1")?.body;
    let exif = exif::Reader::new().read_raw(block.to_vec()).ok()?;
    exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)?
        .value
        .get_uint(0)
}

/// The full-size JPEG preview, located through the sample tables of the first
/// track instead of scanning the file for JPEG markers.
pub fn preview_jpeg(data: &[u8]) -> Option<&[u8]> {
    let moov = find(data, b"moov")?;
    let trak = find(moov.body, b"trak")?;
    let stbl = [b"mdia", b"minf", b"stbl"]
        .iter()
        .try_fold(trak.body, |body, kind| find(body, kind).map(|b| b.body))?;

    // stsz: version/flags, sample_size, sample_count, entry sizes...
    let stsz = find(stbl, b"stsz")?.body;
    let size = match be_u32(stsz, 4)? {
        0 => be_u32(stsz, 12)?,
        s => s,
    } as usize;

    // co64/stco: version/flags, entry_count, chunk offsets...
    let offset = match find(stbl, b"co64") {
        Some(co64) => be_u64(co64.body, 8)?,
        None => be_u32(find(stbl, b"stco")?.body, 8)? as u64,
    };
    let offset = usize::try_from(offset).ok()?;

    let jpeg = data.get(offset..offset.checked_add(size)?)?;
    jpeg.starts_with(&[0xFF, 0xD8]).then_some(jpeg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bmff(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(body);
        out
    }

    fn uuid_box(uuid: &[u8; 16], body: &[u8]) -> Vec<u8> {
        bmff(b"uuid", &[uuid.as_slice(), body].concat())
    }

    /// Little-endian TIFF with a single IFD of SHORT/inline entries.
    fn tiff(entries: &[(u16, u16)]) -> Vec<u8> {
        let mut out = b"II*\0".to_vec();
        out.extend_from_slice(&8u32.to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        for (tag, value) in entries {
            out.extend_from_slice(&tag.to_le_bytes());
            out.extend_from_slice(&3u16.to_le_bytes());
            out.extend_from_slice(&1u32.to_le_bytes());
            out.extend_from_slice(&(*value as u32).to_le_bytes());
        }
        out.extend_from_slice(&0u32.to_le_bytes());
        out
    }

    /// Builds a CR3-shaped file: ftyp, moov{canon uuid{CMT1, CMT2}, trak}, mdat{jpeg}.
    fn sample_cr3(jpeg: &[u8]) -> Vec<u8> {
        let ftyp = bmff(b"ftyp", b"crx \0\0\0\x01crx isom");
        let canon = uuid_box(
            &CANON_UUID,
            &[
                bmff(b"CMT1", &tiff(&[(0x0112, 6)])), // Orientation: rotate 90 CW
                bmff(b"CMT2", &tiff(&[(0x8827, 400)])), // PhotographicSensitivity
            ]
            .concat(),
        );

        let build = |jpeg_offset: u64| {
            let stsz = bmff(
                b"stsz",
                &[0u32, 0, 1, jpeg.len() as u32]
                    .iter()
                    .flat_map(|v| v.to_be_bytes())
                    .collect::<Vec<_>>(),
            );
            let co64 = bmff(
                b"co64",
                &[
                    &[0u8; 4][..],
                    &1u32.to_be_bytes(),
                    &jpeg_offset.to_be_bytes(),
                ]
                .concat(),
            );
            let stbl = bmff(b"stbl", &[stsz, co64].concat());
            let trak = bmff(b"trak", &bmff(b"mdia", &bmff(b"minf", &stbl)));
            let moov = bmff(b"moov", &[canon.clone(), trak].concat());
            [ftyp.clone(), moov].concat()
        };

        // The offset field has a fixed width, so the header length is stable.
        let header_len = build(0).len() as u64 + 8; // + mdat header
        [build(header_len), bmff(b"mdat", jpeg)].concat()
    }

    #[test]
    fn detects_cr3_brand() {
        assert!(is_cr3(&sample_cr3(&[0xFF, 0xD8, 0xFF, 0xD9])));
        assert!(!is_cr3(b"II*\0\x08\0\0\0"));
        assert!(!is_cr3(&[]));
    }

    #[test]
    fn reads_exif_with_correct_ifd_context() {
        let data = sample_cr3(&[0xFF, 0xD8, 0xFF, 0xD9]);
        let exifs = read_exif(&data);
        let field = |tag| {
            exifs
                .iter()
                .find_map(|e| e.get_field(tag, exif::In::PRIMARY))
                .and_then(|f| f.value.get_uint(0))
        };
        assert_eq!(field(exif::Tag::Orientation), Some(6));
        assert_eq!(field(exif::Tag::PhotographicSensitivity), Some(400));
        assert_eq!(orientation(&data), Some(6));
    }

    #[test]
    fn locates_preview_jpeg_from_first_track() {
        let jpeg = [0xFF, 0xD8, 0x01, 0x02, 0x03, 0xFF, 0xD9];
        let data = sample_cr3(&jpeg);
        assert_eq!(preview_jpeg(&data), Some(&jpeg[..]));
    }

    #[test]
    fn malformed_input_yields_nothing() {
        let mut data = sample_cr3(&[0xFF, 0xD8, 0xFF, 0xD9]);
        data.truncate(40);
        assert!(read_exif(&data).is_empty());
        assert!(preview_jpeg(&data).is_none());
        assert!(orientation(&data).is_none());
        // A box claiming to be larger than the file must not panic.
        assert!(preview_jpeg(&[0xFF, 0xFF, 0xFF, 0xFF, b'm', b'o', b'o', b'v']).is_none());
    }

    #[test]
    fn reads_sample_cr3_fixture() {
        let data = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/photos/canon-eos-r50-2025-08-08-canon-eos-r50-2025-08-08-img_8333-56f54f9a-56f54f9a.cr3"
        ))
        .unwrap();
        assert!(is_cr3(&data));
        let exifs = read_exif(&data);
        let make = exifs
            .iter()
            .find_map(|e| e.get_field(exif::Tag::Make, exif::In::PRIMARY))
            .map(|f| f.display_value().to_string());
        assert_eq!(make.as_deref(), Some("\"Canon\""));
        assert!(exifs.iter().any(|e| e
            .get_field(exif::Tag::ExposureTime, exif::In::PRIMARY)
            .is_some()));
        assert!(orientation(&data).is_some());
        let jpeg = preview_jpeg(&data).unwrap();
        assert!(jpeg.ends_with(&[0xFF, 0xD9]));
    }
}
