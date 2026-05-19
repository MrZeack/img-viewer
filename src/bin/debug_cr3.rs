use std::fs::File;
use std::io::Write;
use std::path::Path;

fn main() {
    let path = Path::new(
        "./photos/canon-eos-r50-2025-08-08-canon-eos-r50-2025-08-08-img_8333-56f54f9a-56f54f9a.cr3",
    );

    println!("Reading file: {:?}", path);
    let data = std::fs::read(path).expect("Failed to read file");
    println!("File size: {} bytes", data.len());

    let mut jpegs = Vec::new();
    let mut i = 0;
    while i < data.len() - 2 {
        // Search for JPEG SOI (Start of Image): FF D8 FF
        if data[i] == 0xFF && data[i + 1] == 0xD8 && data[i + 2] == 0xFF {
            let start = i;
            // Search for EOI (End of Image): FF D9
            // We search forward from start
            let mut end = 0;
            let mut j = start + 2;
            while j < data.len() - 1 {
                if data[j] == 0xFF && data[j + 1] == 0xD9 {
                    end = j + 2;
                    // Found a potential JPEG
                    // Simple validation: check if it looks like a reasonable JPEG
                    // (at least some size)
                    if end - start > 1000 {
                        jpegs.push((start, end, end - start));
                    }
                    // For nested JPEGs (unlikely in raw streams usually, but possible),
                    // we might want to continue searching or break.
                    // Usually we want the outermost or sequential ones.
                    // Let's assume sequential for now.
                    i = end - 1;
                    break;
                }
                j += 1;
            }
            if end == 0 {
                // No EOI found, break inner loop, increment i
                i += 1;
            }
        } else {
            i += 1;
        }
    }

    println!("Found {} potential JPEG candidates.", jpegs.len());
    for (idx, (start, _end, size)) in jpegs.iter().enumerate() {
        println!("  Candidate {}: Offset {}, Size {} bytes", idx, start, size);
    }

    if let Some((best_start, best_end, best_size)) = jpegs.iter().max_by_key(|(_, _, size)| size) {
        println!(
            "Largest JPEG is at offset {} with size {}",
            best_start, best_size
        );

        let jpg_data = &data[*best_start..*best_end];

        match image::load_from_memory_with_format(jpg_data, image::ImageFormat::Jpeg) {
            Ok(img) => {
                println!(
                    "Successfully loaded largest JPEG! Dimensions: {}x{}",
                    img.width(),
                    img.height()
                );
                // Save it
                let mut file =
                    File::create("extracted_preview.jpg").expect("Failed to create file");
                file.write_all(jpg_data).expect("Failed to write data");
                println!("Saved extracted_preview.jpg");
            }
            Err(e) => println!("Failed to load largest JPEG candidate: {}", e),
        }
    } else {
        println!("No suitable JPEG found.");
    }
}
