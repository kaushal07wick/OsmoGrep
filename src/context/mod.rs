pub mod indexer;
pub enum ContextEvent {
    Started,
    Finished,
    Error(String),
}

use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::thread;

use crate::context::indexer::load_or_build;

pub fn spawn_indexer(root: PathBuf, tx: Sender<ContextEvent>) {
    thread::spawn(move || {
        let _ = tx.send(ContextEvent::Started);

        let result = std::panic::catch_unwind(|| load_or_build(&root));

        match result {
            Ok(_) => {
                let _ = tx.send(ContextEvent::Finished);
            }
            Err(_) => {
                let _ = tx.send(ContextEvent::Error("context indexing panicked".into()));
            }
        }
    });
}
