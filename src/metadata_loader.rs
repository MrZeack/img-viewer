use std::path::{Path, PathBuf};
use std::sync::mpsc;
use rayon::prelude::*;

use crate::metadata_simple::extract_metadata;
use crate::event::{EventBus, ViewerEvent};

pub struct MetadataLoader {
    sender: mpsc::Sender<PathBuf>,
}

impl MetadataLoader {
    pub fn new(num_threads: usize, event_bus: EventBus) -> Self {
        let (sender, task_receiver) = mpsc::channel::<PathBuf>();

        // Use Rayon's thread pool to process metadata extraction
        std::thread::spawn(move || {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(num_threads)
                .build()
                .expect("Failed to build Rayon thread pool");
            
            pool.install(|| {
                task_receiver.into_iter().par_bridge().for_each(|path| {
                    if let Ok(metadata) = extract_metadata(&path) {
                        event_bus.publish(ViewerEvent::MetadataReady(path, metadata));
                    }
                });
            });
        });

        Self { sender }
    }

    pub fn load_metadata(&self, path: &Path) {
        let _ = self.sender.send(path.to_path_buf());
    }
}
