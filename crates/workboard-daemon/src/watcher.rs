use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

use crate::protocol::WriteCommand;
use crate::server::{WriterRequest, send_to_writer};

#[derive(Debug, Clone)]
pub struct WatchConfig {
    pub claude_root: Option<PathBuf>,
    pub codex_root: Option<PathBuf>,
    pub debounce: Duration,
    pub reconcile_interval: Duration,
}

impl WatchConfig {
    pub fn new(claude_root: Option<PathBuf>, codex_root: Option<PathBuf>) -> Self {
        Self {
            claude_root,
            codex_root,
            debounce: Duration::from_millis(500),
            reconcile_interval: Duration::from_secs(60),
        }
    }

    fn roots(&self) -> impl Iterator<Item = &Path> {
        self.claude_root
            .iter()
            .chain(self.codex_root.iter())
            .map(PathBuf::as_path)
    }

    fn refresh_command(&self) -> WriteCommand {
        WriteCommand::RefreshNativeSessions {
            claude_root: self.claude_root.clone(),
            codex_root: self.codex_root.clone(),
        }
    }
}

pub(crate) fn watch_loop(
    config: WatchConfig,
    writer: Sender<WriterRequest>,
    stopping: Arc<AtomicBool>,
) {
    let (event_sender, event_receiver) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(event_sender).ok();
    let mut watched = HashSet::new();
    register_roots(&config, watcher.as_mut(), &mut watched);
    refresh(&config, &writer);
    let mut dirty_since = None;
    let mut next_reconcile = Instant::now() + config.reconcile_interval;

    while !stopping.load(Ordering::Acquire) {
        let timeout = next_timeout(dirty_since, next_reconcile, config.debounce);
        match event_receiver.recv_timeout(timeout) {
            Ok(Ok(event)) if is_relevant(&event) => {
                dirty_since = Some(Instant::now());
            }
            Ok(Err(_)) => {
                dirty_since = Some(Instant::now());
            }
            Ok(Ok(_)) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => watcher = None,
        }

        let now = Instant::now();
        if dirty_since.is_some_and(|since| now.duration_since(since) >= config.debounce) {
            refresh(&config, &writer);
            dirty_since = None;
        }
        if now >= next_reconcile {
            register_roots(&config, watcher.as_mut(), &mut watched);
            refresh(&config, &writer);
            next_reconcile = now + config.reconcile_interval;
        }
    }
}

fn register_roots(
    config: &WatchConfig,
    watcher: Option<&mut RecommendedWatcher>,
    watched: &mut HashSet<PathBuf>,
) {
    let Some(watcher) = watcher else {
        return;
    };
    for root in config.roots().filter(|root| root.is_dir()) {
        let root = root.to_path_buf();
        if watched.insert(root.clone()) && watcher.watch(&root, RecursiveMode::Recursive).is_err() {
            watched.remove(&root);
        }
    }
}

fn refresh(config: &WatchConfig, writer: &Sender<WriterRequest>) {
    let _ = send_to_writer(config.refresh_command(), writer);
}

fn next_timeout(
    dirty_since: Option<Instant>,
    next_reconcile: Instant,
    debounce: Duration,
) -> Duration {
    let now = Instant::now();
    let reconcile = next_reconcile.saturating_duration_since(now);
    dirty_since
        .map(|since| {
            (since + debounce)
                .saturating_duration_since(now)
                .min(reconcile)
        })
        .unwrap_or(reconcile)
        .min(Duration::from_millis(100))
}

fn is_relevant(event: &Event) -> bool {
    event.paths.is_empty()
        || event
            .paths
            .iter()
            .any(|path| path.is_dir() || path.extension().is_some_and(|value| value == "jsonl"))
}
