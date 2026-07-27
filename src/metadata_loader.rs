use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use crate::event::{EventBus, ViewerEvent};
use crate::metadata_simple::extract_metadata;

pub struct MetadataLoader {
    target: Arc<Mutex<Option<(u64, PathBuf)>>>,
    condvar: Arc<Condvar>,
    next_request_id: Arc<AtomicU64>,
}

impl MetadataLoader {
    pub fn new(_num_threads: usize, event_bus: EventBus) -> Self {
        let target: Arc<Mutex<Option<(u64, PathBuf)>>> = Arc::new(Mutex::new(None));
        let condvar = Arc::new(Condvar::new());
        let next_request_id = Arc::new(AtomicU64::new(0));

        let t = target.clone();
        let c = condvar.clone();
        let latest_request_id = next_request_id.clone();

        std::thread::spawn(move || {
            loop {
                // Wait for a new task
                let (request_id, path): (u64, PathBuf) = {
                    let mut lock = t.lock().unwrap();
                    while lock.is_none() {
                        lock = c.wait(lock).unwrap();
                    }
                    lock.take().unwrap()
                };

                if let Ok(metadata) = extract_metadata(&path) {
                    if request_id == latest_request_id.load(Ordering::Acquire) {
                        event_bus
                            .publish(ViewerEvent::MetadataReady(path.clone(), Box::new(metadata)));
                    }
                }
            }
        });

        Self {
            target,
            condvar,
            next_request_id,
        }
    }

    pub fn load_metadata(&self, path: &Path) {
        let mut lock = self.target.lock().unwrap();
        let request_id = self.next_request_id.fetch_add(1, Ordering::AcqRel) + 1;
        *lock = Some((request_id, path.to_path_buf()));
        self.condvar.notify_one();
    }
}
