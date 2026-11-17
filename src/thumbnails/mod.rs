//! Miniaturas para la vista de cuadrícula: generación, índice en disco y cola priorizada.
pub mod generate;
pub mod index;
pub mod queue;
pub mod service;

use std::path::{Path, PathBuf};

/// Lado mayor de la miniatura en píxeles: nítida en Retina hasta celdas de ~190 pt.
pub const THUMB_MAX: u32 = 384;

/// Carpeta de caché del índice para `dir`: `<cache del usuario>/thumbs/<hash de la ruta>`.
pub fn cache_dir_for(dir: &Path) -> Option<PathBuf> {
    let canonical = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let base = directories::ProjectDirs::from("fyi", "voza", "img-viewer")?;
    Some(cache_dir_in(base.cache_dir(), &canonical))
}

fn cache_dir_in(base: &Path, canonical_dir: &Path) -> PathBuf {
    let key = fnv1a64(canonical_dir.to_string_lossy().as_bytes());
    base.join("thumbs").join(format!("{key:016x}"))
}

/// Hilos de generación: la mitad de los núcleos. El servicio corre desde que se abre la
/// carpeta, así que deja margen para la carga de la imagen actual y la UI.
pub fn worker_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get() / 2)
        .unwrap_or(2)
        .max(1)
}

/// FNV-1a de 64 bits: estable entre versiones de Rust, a diferencia de `DefaultHasher`.
fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, &b| {
        (hash ^ b as u64).wrapping_mul(0x100000001b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a64_matches_reference_vectors() {
        assert_eq!(fnv1a64(b""), 0xcbf29ce484222325);
        assert_eq!(fnv1a64(b"a"), 0xaf63dc4c8601ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x85944171f73967e8);
    }

    #[test]
    fn cache_dir_is_stable_per_folder() {
        let a = cache_dir_in(Path::new("/c"), Path::new("/fotos/viaje"));
        assert_eq!(a, cache_dir_in(Path::new("/c"), Path::new("/fotos/viaje")));
        assert_ne!(a, cache_dir_in(Path::new("/c"), Path::new("/fotos/otro")));
        assert!(a.starts_with("/c/thumbs"));
    }
}
