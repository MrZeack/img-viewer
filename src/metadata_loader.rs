use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use rayon::prelude::*;

use crate::metadata_simple::{extract_metadata, ImageMetadata};

pub struct MetadataLoader {
    sender: mpsc::Sender<PathBuf>,
    receiver: mpsc::Receiver<MetadataResult>,
}

pub struct MetadataResult {
    pub path: PathBuf,
    pub result: Result<ImageMetadata>,
}

impl MetadataLoader {
    pub fn new() -> Self {
        let (sender, task_receiver) = mpsc::channel::<PathBuf>();
        let (result_sender, receiver) = mpsc::channel::<MetadataResult>();

        // Use Rayon's thread pool to process metadata extraction
        std::thread::spawn(move || {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(2) // A small number of threads is enough for metadata
                .build()
                .unwrap();
            
            pool.install(|| {
                task_receiver.into_iter().par_bridge().for_each(|path| {
                    let metadata_result = extract_metadata(&path);
                    let result = MetadataResult {
                        path: path.clone(),
                        result: metadata_result,
                    };
                    // The sender can be cloned to be used in parallel threads
                    if result_sender.send(result).is_err() {
                        // The receiver has been dropped, so we can stop.
                        return;
                    }
                });
            });
        });

        Self { sender, receiver }
    }

    pub fn load_metadata(&self, path: &Path) {
        let _ = self.sender.send(path.to_path_buf());
    }

    pub fn try_recv_result(&self) -> Option<MetadataResult> {
        self.receiver.try_recv().ok()
    }
}
