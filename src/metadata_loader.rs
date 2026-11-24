use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use crate::event::{EventBus, ViewerEvent};
use crate::metadata_simple::extract_metadata;

pub struct MetadataLoader {
    target: Arc<Mutex<Option<(u64, PathBuf)>>>,
    condvar: Arc<Condvar>,
    next_request_id: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl MetadataLoader {
    pub fn new(event_bus: EventBus) -> Self {
        let target: Arc<Mutex<Option<(u64, PathBuf)>>> = Arc::new(Mutex::new(None));
        let condvar = Arc::new(Condvar::new());
        let next_request_id = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));

        let t = Arc::clone(&target);
        let c = Arc::clone(&condvar);
        let latest_request_id = Arc::clone(&next_request_id);
        let stop_flag = Arc::clone(&stop);

        let worker = std::thread::Builder::new()
            .name("meta-load".into())
            .spawn(move || loop {
                let (request_id, path): (u64, PathBuf) = {
                    let mut lock = t.lock().unwrap_or_else(|e| e.into_inner());
                    loop {
                        if stop_flag.load(Ordering::SeqCst) {
                            return;
                        }
                        if let Some(item) = lock.take() {
                            break item;
                        }
                        lock = c.wait(lock).unwrap_or_else(|e| e.into_inner());
                    }
                };

                if let Ok(metadata) = extract_metadata(&path) {
                    if request_id == latest_request_id.load(Ordering::Acquire) {
                        event_bus
                            .publish(ViewerEvent::MetadataReady(path.clone(), Box::new(metadata)));
                    }
                }
            })
            .ok();

        Self {
            target,
            condvar,
            next_request_id,
            stop,
            worker,
        }
    }

    pub fn load_metadata(&self, path: &Path) {
        let mut lock = self.target.lock().unwrap_or_else(|e| e.into_inner());
        let request_id = self.next_request_id.fetch_add(1, Ordering::AcqRel) + 1;
        *lock = Some((request_id, path.to_path_buf()));
        self.condvar.notify_one();
    }
}

impl Drop for MetadataLoader {
    fn drop(&mut self) {
        {
            // Con el candado tomado, el aviso no cae entre la comprobación del hilo y su `wait`.
            let _target = self.target.lock().unwrap_or_else(|e| e.into_inner());
            self.stop.store(true, Ordering::SeqCst);
        }
        self.condvar.notify_all();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
