//! In-flight jobs so RevokeJob can kill the process.

use std::sync::LazyLock;

use dashmap::DashMap;
use tokio::sync::oneshot;
use uuid::Uuid;

static JOBS: LazyLock<DashMap<Uuid, oneshot::Sender<()>>> = LazyLock::new(DashMap::new);

pub fn register(id: Uuid, tx: oneshot::Sender<()>) {
    JOBS.insert(id, tx);
}

pub fn cancel(id: Uuid) -> bool {
    JOBS.remove(&id)
        .map(|(_, tx)| tx.send(()).is_ok())
        .unwrap_or(false)
}

pub fn remove(id: &Uuid) {
    JOBS.remove(id);
}
