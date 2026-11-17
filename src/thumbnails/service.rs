//! Servicio de miniaturas: workers en segundo plano sobre una cola priorizada y el índice en disco.
//!
//! La lista de archivos puede cambiar (mover, deshacer) sin reiniciar los workers: `set_files`
//! reemplaza la cola y sube la época; los trabajos de la época anterior se descartan al terminar.
use super::generate::{decode_thumb, encode_jpeg, generate, to_color_image};
use super::index::{FileStamp, ThumbIndex};
use super::queue::{Job, Queue};
use egui::ColorImage;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;

/// Cada cuántas miniaturas nuevas se persiste `index.bin`.
const SAVE_EVERY: usize = 200;

#[derive(Debug)]
pub struct ThumbResult {
    pub path: PathBuf,
    /// `None` si no se pudo generar.
    pub image: Option<ColorImage>,
}

pub struct ThumbnailService {
    shared: Arc<Shared>,
    results: Receiver<ThumbResult>,
    workers: Vec<JoinHandle<()>>,
}

struct State {
    epoch: u64,
    files: Arc<Vec<PathBuf>>,
    queue: Queue,
}

struct Shared {
    thumb_max: u32,
    state: Mutex<State>,
    work: Condvar,
    stop: AtomicBool,
    index: Option<Mutex<ThumbIndex>>,
    repaint: Option<egui::Context>,
}

impl Shared {
    fn queue_for(&self, files: &[PathBuf]) -> Queue {
        let indexed = match &self.index {
            Some(index) => {
                let index = lock(index);
                files.iter().map(|f| index.contains(&file_key(f))).collect()
            }
            None => vec![false; files.len()],
        };
        Queue::new(indexed)
    }
}

impl ThumbnailService {
    pub fn start(
        files: Vec<PathBuf>,
        cache_dir: Option<PathBuf>,
        thumb_max: u32,
        threads: usize,
        repaint: Option<egui::Context>,
    ) -> Self {
        let index = cache_dir.and_then(|dir| match ThumbIndex::open(&dir, thumb_max) {
            Ok(index) => Some(index),
            Err(e) => {
                eprintln!("Thumbnail cache disabled ({}): {e}", dir.display());
                None
            }
        });
        let mut shared = Shared {
            thumb_max,
            state: Mutex::new(State {
                epoch: 0,
                files: Arc::new(Vec::new()),
                queue: Queue::new(Vec::new()),
            }),
            work: Condvar::new(),
            stop: AtomicBool::new(false),
            index: index.map(Mutex::new),
            repaint,
        };
        let queue = shared.queue_for(&files);
        let state = shared.state.get_mut().unwrap_or_else(|e| e.into_inner());
        state.files = Arc::new(files);
        state.queue = queue;

        let shared = Arc::new(shared);
        let (tx, results) = mpsc::channel();
        let workers = (0..threads.max(1))
            .map(|n| {
                let shared = shared.clone();
                let tx = tx.clone();
                std::thread::Builder::new()
                    .name(format!("thumb-{n}"))
                    .spawn(move || worker(&shared, &tx))
                    .expect("spawn thumbnail worker")
            })
            .collect();

        Self {
            shared,
            results,
            workers,
        }
    }

    /// Reemplaza la lista (misma carpeta) conservando workers e índice.
    pub fn set_files(&self, files: Vec<PathBuf>) {
        let queue = self.shared.queue_for(&files);
        let mut state = lock(&self.shared.state);
        state.epoch += 1;
        state.files = Arc::new(files);
        state.queue = queue;
        drop(state);
        self.shared.work.notify_all();
    }

    /// `focus` prioriza el índice cercano; `wanted` son las miniaturas que la UI quiere ver.
    pub fn set_view(&self, focus: usize, wanted: impl IntoIterator<Item = usize>) {
        let mut state = lock(&self.shared.state);
        state.queue.set_focus(focus);
        state.queue.set_wanted(wanted);
        drop(state);
        self.shared.work.notify_all();
    }

    pub fn try_recv(&self) -> Option<ThumbResult> {
        self.results.try_recv().ok()
    }

    pub fn progress(&self) -> (usize, usize) {
        lock(&self.shared.state).queue.progress()
    }

    #[cfg(test)]
    pub fn is_failed(&self, i: usize) -> bool {
        lock(&self.shared.state).queue.is_failed(i)
    }
}

impl Drop for ThumbnailService {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        self.shared.work.notify_all();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
        if let Some(index) = &self.shared.index {
            if let Err(e) = lock(index).save() {
                eprintln!("Failed to save thumbnail index: {e}");
            }
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn file_key(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn worker(shared: &Shared, tx: &Sender<ThumbResult>) {
    loop {
        let (job, path, epoch) = {
            let mut state = lock(&shared.state);
            loop {
                if shared.stop.load(Ordering::SeqCst) {
                    return;
                }
                if let Some(job) = state.queue.next() {
                    break (job, state.files[job.index()].clone(), state.epoch);
                }
                state = shared.work.wait(state).unwrap_or_else(|e| e.into_inner());
            }
        };

        // Un archivo malformado no debe tumbar el worker (release usa panic = "unwind").
        let display = matches!(job, Job::Display(_));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            process(shared, &path, display)
        }))
        .unwrap_or_else(|_| Err("panic al generar miniatura".into()));

        let mut state = lock(&shared.state);
        if state.epoch == epoch {
            state.queue.finish(job, result.is_ok());
        }
        drop(state);

        if display {
            let image = result.ok().flatten();
            if tx.send(ThumbResult { path, image }).is_err() {
                return;
            }
            if let Some(ctx) = &shared.repaint {
                ctx.request_repaint();
            }
        }
    }
}

/// Devuelve la imagen solo si `display`; si no, solo asegura la entrada en disco.
fn process(shared: &Shared, path: &Path, display: bool) -> Result<Option<ColorImage>, String> {
    let key = file_key(path);
    let stamp = FileStamp::of(path).map_err(|e| e.to_string())?;

    if let Some(index) = &shared.index {
        let cached = {
            let index = lock(index);
            index
                .lookup(&key, stamp)
                .and_then(|(offset, len)| index.read(offset, len).ok())
        };
        if let Some(bytes) = cached {
            if !display {
                return Ok(None);
            }
            // Un blob dañado se regenera abajo.
            if let Ok(image) = decode_thumb(&bytes) {
                return Ok(Some(image));
            }
        }
    }

    let rgb = generate(path, shared.thumb_max)?;
    if let Some(index) = &shared.index {
        let jpeg = encode_jpeg(&rgb)?;
        let mut index = lock(index);
        if let Err(e) = index.insert(&key, stamp, &jpeg) {
            eprintln!("Failed to store thumbnail for {}: {e}", path.display());
        } else if index.unsaved() >= SAVE_EVERY {
            let _ = index.save();
        }
    }
    Ok(display.then(|| to_color_image(&rgb)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn setup(tag: &str, n: u32) -> (PathBuf, Vec<PathBuf>) {
        let root = std::env::temp_dir().join(format!("ivt-svc-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let photos = root.join("photos");
        std::fs::create_dir_all(&photos).unwrap();
        let mut files = Vec::new();
        for i in 0..n {
            let img = image::RgbImage::from_pixel(800, 600, image::Rgb([i as u8 * 40, 10, 10]));
            let path = photos.join(format!("img{i}.jpg"));
            std::fs::write(&path, encode_jpeg(&img).unwrap()).unwrap();
            files.push(path);
        }
        std::fs::write(photos.join("broken.jpg"), b"nope").unwrap();
        files.push(photos.join("broken.jpg"));
        (root, files)
    }

    fn wait_until(service: &ThumbnailService, mut done: impl FnMut(&ThumbnailService) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !done(service) {
            assert!(Instant::now() < deadline, "timeout");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn collect(service: &ThumbnailService, expected: usize) -> Vec<ThumbResult> {
        let mut out = Vec::new();
        wait_until(service, |s| {
            while let Some(r) = s.try_recv() {
                out.push(r);
            }
            out.len() >= expected
        });
        out
    }

    #[test]
    fn wanted_thumbnails_are_delivered_and_failures_reported() {
        let (root, files) = setup("deliver", 3);
        let service = ThumbnailService::start(files, Some(root.join("cache")), 64, 2, None);
        service.set_view(0, [0, 1, 2, 3]);
        let mut results = collect(&service, 4);
        results.sort_by(|a, b| a.path.cmp(&b.path));
        assert!(results[0].path.ends_with("broken.jpg"));
        assert!(results[0].image.is_none());
        for r in &results[1..] {
            assert_eq!(r.image.as_ref().unwrap().size, [64, 48]);
        }
        assert!(service.is_failed(3));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn whole_folder_is_indexed_in_background_and_reused_on_restart() {
        let (root, files) = setup("restart", 4);
        let cache = root.join("cache");
        {
            let service = ThumbnailService::start(files.clone(), Some(cache.clone()), 64, 2, None);
            wait_until(&service, |s| s.progress() == (5, 5));
            assert!(service.try_recv().is_none(), "indexar no envía imágenes");
        }
        let service = ThumbnailService::start(files, Some(cache.clone()), 64, 2, None);
        assert_eq!(service.progress().0, 4, "las 4 válidas ya están indexadas");
        service.set_view(0, [2]);
        let results = collect(&service, 1);
        assert!(results[0].path.ends_with("img2.jpg"));
        assert!(results[0].image.is_some());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn set_files_remaps_indices_without_restarting() {
        let (root, files) = setup("setfiles", 3);
        let service = ThumbnailService::start(files.clone(), Some(root.join("cache")), 64, 2, None);
        wait_until(&service, |s| s.progress() == (4, 4));

        let remaining = vec![files[0].clone(), files[2].clone()];
        service.set_files(remaining);
        assert_eq!(service.progress(), (2, 2));
        service.set_view(0, [1]);
        let results = collect(&service, 1);
        assert!(results[0].path.ends_with("img2.jpg"));
        assert!(!service.is_failed(1));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn works_without_cache_dir() {
        let (root, files) = setup("nocache", 1);
        let service = ThumbnailService::start(files, None, 64, 1, None);
        service.set_view(0, [0]);
        assert!(collect(&service, 1)[0].image.is_some());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dropping_service_stops_workers_promptly() {
        let (root, files) = setup("drop", 2);
        let service = ThumbnailService::start(files, Some(root.join("cache")), 64, 2, None);
        let t = Instant::now();
        drop(service);
        assert!(t.elapsed() < Duration::from_secs(5));
        std::fs::remove_dir_all(root).unwrap();
    }

    /// `IMG_BENCH_DIR=<carpeta> cargo test --release bench_service -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn bench_service() {
        let dir = PathBuf::from(std::env::var("IMG_BENCH_DIR").expect("IMG_BENCH_DIR"));
        let files = crate::image_loader::get_image_files(&dir, true).unwrap();
        let cache = std::env::temp_dir().join(format!("ivt-bench-{}", std::process::id()));
        let threads = crate::thumbnails::worker_threads();

        let t = Instant::now();
        let service =
            ThumbnailService::start(files.clone(), Some(cache.clone()), 384, threads, None);
        service.set_view(0, 0..60);
        let first = collect(&service, 60);
        println!("primera pantalla (60, en frío): {:.2?}", t.elapsed());
        assert_eq!(first.len(), 60);
        wait_until(&service, |s| s.progress().0 == s.progress().1);
        let total = t.elapsed();
        drop(service);
        println!(
            "índice completo: {} archivos en {:.2?} con {threads} hilos ({:.1} ms/archivo)",
            files.len(),
            total,
            total.as_secs_f64() * 1000.0 / files.len() as f64
        );
        let size: u64 = std::fs::read_dir(&cache)
            .unwrap()
            .flatten()
            .map(|e| e.metadata().unwrap().len())
            .sum();
        println!("tamaño del caché: {:.1} MB", size as f64 / 1e6);

        let t = Instant::now();
        let service =
            ThumbnailService::start(files.clone(), Some(cache.clone()), 384, threads, None);
        let opened = t.elapsed();
        service.set_view(700, 700..760);
        collect(&service, 60);
        println!(
            "reabrir: {opened:.2?}; 60 miniaturas desde el índice: {:.2?}",
            t.elapsed()
        );
        drop(service);
        std::fs::remove_dir_all(cache).unwrap();
    }
}
