// weaveback-serve/src/server/watcher.rs
// I'd Really Rather You Didn't edit this generated file.

use super::*;

pub(in crate::server) type SseSenders = Arc<Mutex<Vec<std::sync::mpsc::SyncSender<()>>>>;
pub(in crate::server) type ReloadVersion = Arc<AtomicU64>;

pub(in crate::server) fn spawn_watcher(watch_dir: PathBuf, senders: SseSenders, version: ReloadVersion) {
    thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = match notify::recommended_watcher(tx) {
            Ok(w) => w,
            Err(e) => { eprintln!("wb-serve: watcher error: {e}"); return; }
        };
        if let Err(e) = watcher.watch(&watch_dir, RecursiveMode::Recursive) {
            eprintln!("wb-serve: watch error: {e}");
            return;
        }
        for result in &rx {
            if result.is_ok() {
                version.fetch_add(1, Ordering::Relaxed);
                let mut locked = senders.lock().unwrap();
                locked.retain(|s| s.send(()).is_ok());
            }
        }
        drop(watcher);
    });
}

