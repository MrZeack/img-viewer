//! Escaneo de carpeta en segundo plano: la ventana responde y muestra un indicador mientras tanto.
use super::state::ImageViewerApp;
use crate::image_loader::scan_image_files;
use eframe::egui::Context;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::{Instant, SystemTime};

type ScanResult = Result<Vec<(PathBuf, SystemTime)>, String>;

pub(crate) struct FolderLoad {
    pub(crate) dir: PathBuf,
    pub(crate) started: Instant,
    /// Archivos de imagen encontrados hasta ahora.
    pub(crate) found: Arc<AtomicUsize>,
    receiver: Receiver<ScanResult>,
}

impl FolderLoad {
    pub(crate) fn found(&self) -> usize {
        self.found.load(Ordering::Relaxed)
    }
}

impl ImageViewerApp {
    pub(crate) fn is_loading(&self) -> bool {
        self.folder_load.is_some()
    }

    /// Empieza a escanear `dir`; la lista actual se vacía hasta que termine.
    pub(crate) fn start_folder_load(&mut self, dir: PathBuf) {
        let found = Arc::new(AtomicUsize::new(0));
        let (tx, receiver) = mpsc::channel();
        let (scan_dir, counter) = (dir.clone(), found.clone());
        let spawned = std::thread::Builder::new()
            .name("folder-scan".into())
            .spawn(move || {
                let result = scan_image_files(&scan_dir, Some(&counter)).map_err(|e| e.to_string());
                let _ = tx.send(result);
            });
        if let Err(e) = spawned {
            eprintln!("Failed to start folder scan: {e}");
        }

        self.current_dir = dir.clone();
        self.image_files.clear();
        self.mtimes.clear();
        self.current_index = 0;
        self.current_image = None;
        self.current_load_error = None;
        self.current_metadata = None;
        self.folder_load = Some(FolderLoad {
            dir,
            started: Instant::now(),
            found,
            receiver,
        });
    }

    /// Aplica el resultado si el escaneo terminó; devuelve `true` en ese frame.
    pub(crate) fn poll_folder_load(&mut self, ctx: &Context) -> bool {
        let Some(load) = &self.folder_load else {
            return false;
        };
        let result = match load.receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => Err("el escaneo terminó sin resultado".into()),
        };
        self.folder_load = None;
        let entries = result.unwrap_or_else(|e| {
            eprintln!("Failed to read {}: {e}", self.current_dir.display());
            Vec::new()
        });
        self.mtimes = entries.iter().cloned().collect();
        self.image_files = entries.into_iter().map(|(path, _)| path).collect();
        self.apply_sort();
        self.current_index = 0;
        if !self.image_files.is_empty() {
            self.load_current_image(ctx);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::time::Duration;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ivt-load-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(dir: &std::path::Path, name: &str, secs: u64) {
        let file = File::create(dir.join(name)).unwrap();
        file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000 + secs))
            .unwrap();
    }

    fn wait_loaded(app: &mut ImageViewerApp) {
        let ctx = Context::default();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !app.poll_folder_load(&ctx) {
            assert!(Instant::now() < deadline, "timeout");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn loads_in_background_sorted_by_date_ascending() {
        let dir = temp_dir("sorted");
        touch(&dir, "new.jpg", 300);
        touch(&dir, "old.jpg", 100);
        touch(&dir, "mid.png", 200);
        touch(&dir, "notes.txt", 50);

        let mut app = crate::app::state::test_app(vec![PathBuf::from("/stale.jpg")], 0);
        app.start_folder_load(dir.clone());
        assert!(app.is_loading());
        assert!(app.image_files.is_empty(), "la lista anterior se descarta");
        wait_loaded(&mut app);

        assert!(!app.is_loading());
        assert_eq!(app.current_dir, dir);
        let names: Vec<_> = app
            .image_files
            .iter()
            .map(|p| p.file_name().unwrap().to_owned())
            .collect();
        assert_eq!(names, ["old.jpg", "mid.png", "new.jpg"]);
        assert_eq!(app.mtimes.len(), 3);
        assert_eq!(app.current_index, 0);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_folder_finishes_with_empty_list() {
        let mut app = crate::app::state::test_app(vec![], 0);
        app.start_folder_load(PathBuf::from("/definitely/not/here"));
        wait_loaded(&mut app);
        assert!(app.image_files.is_empty());
        assert!(!app.is_loading());
    }
}
