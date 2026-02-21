use std::path::Path;
use std::sync::mpsc::Sender;

use anyhow::Result;
use notify::{EventKind, RecursiveMode, Watcher, recommended_watcher};

pub fn setup(path: &Path, tx: Sender<()>) -> Result<impl Watcher> {
    let mut watcher = recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            if matches!(event.kind, EventKind::Modify(_)) {
                let _ = tx.send(());
            }
        }
    })?;
    watcher.watch(path, RecursiveMode::NonRecursive)?;
    Ok(watcher)
}
