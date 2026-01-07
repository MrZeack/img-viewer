use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Condvar};

use crate::metadata_simple::extract_metadata;
use crate::event::{EventBus, ViewerEvent};

pub struct MetadataLoader {
    target: Arc<Mutex<Option<PathBuf>>>,
    condvar: Arc<Condvar>,
}

impl MetadataLoader {
    pub fn new(_num_threads: usize, event_bus: EventBus) -> Self {
        let target = Arc::new(Mutex::new(None));
        let condvar = Arc::new(Condvar::new());
        
        let t = target.clone();
        let c = condvar.clone();
        
        std::thread::spawn(move || {
            let mut current_path: Option<PathBuf> = None;
            
            loop {
                // Wait for a new task
                let path = {
                    let mut lock = t.lock().unwrap();
                    while lock.is_none() {
                        lock = c.wait(lock).unwrap();
                    }
                    lock.take().unwrap()
                };
                
                // Check if we are already processing this path (debounce)
                if Some(&path) == current_path.as_ref() {
                     continue;
                }
                current_path = Some(path.clone());

                // Check again before expensive operation if a new task arrived?
                // We can peek at the lock.
                if let Ok(lock) = t.try_lock() {
                    if lock.is_some() {
                        continue; // Skip this one, a newer one is waiting
                    }
                }
                
                if let Ok(metadata) = extract_metadata(&path) {
                    // One final check if we should discard result? 
                    // No, let the app decide. But we can check if a new request is pending.
                    // If a new request is pending, this result is likely obsolete.
                    let obsolete = if let Ok(lock) = t.try_lock() {
                        lock.is_some()
                    } else {
                        false
                    };
                    
                    if !obsolete {
                        event_bus.publish(ViewerEvent::MetadataReady(path.clone(), metadata));
                    }
                }
            }
        });

        Self { target, condvar }
    }

    pub fn load_metadata(&self, path: &Path) {
        let mut lock = self.target.lock().unwrap();
        *lock = Some(path.to_path_buf());
        self.condvar.notify_one();
    }
}
