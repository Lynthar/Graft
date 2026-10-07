//! Long-running work (preview, execution) runs as background tasks the UI polls.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tokio::sync::Notify;

/// Finished tasks kept for polling; older ones are dropped first.
const KEEP_FINISHED: usize = 20;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Running,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub phase: String,
    pub done: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub id: String,
    pub kind: &'static str,
    pub status: Status,
    pub progress: Progress,
    pub error: Option<String>,
    pub result: Option<serde_json::Value>,
}

pub struct Task {
    pub id: String,
    state: Mutex<Snapshot>,
    cancel_requested: AtomicBool,
    cancel_notify: Notify,
}

impl Task {
    pub fn progress(&self, phase: &str, done: usize, total: usize) {
        self.state.lock().expect("task state poisoned").progress =
            Progress { phase: phase.to_string(), done, total };
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel_requested.load(Ordering::SeqCst)
    }

    /// Resolves once cancellation is requested; race it against waits that can be long.
    pub async fn cancelled(&self) {
        loop {
            let notified = self.cancel_notify.notified();
            if self.is_cancelled() {
                return;
            }
            notified.await;
        }
    }

    fn request_cancel(&self) {
        self.cancel_requested.store(true, Ordering::SeqCst);
        self.cancel_notify.notify_waiters();
    }

    pub fn snapshot(&self) -> Snapshot {
        self.state.lock().expect("task state poisoned").clone()
    }

    fn finish(&self, status: Status, error: Option<String>, result: Option<serde_json::Value>) {
        let mut state = self.state.lock().expect("task state poisoned");
        state.status = status;
        state.error = error;
        state.result = result;
    }
}

#[derive(Default)]
pub struct TaskRegistry {
    tasks: Mutex<VecDeque<Arc<Task>>>,
}

impl TaskRegistry {
    /// Start `work` in the background. Its `Ok` value becomes the task result; a task
    /// whose cancellation was requested ends as `Cancelled` and keeps any partial result.
    pub fn spawn<F, Fut, T>(self: &Arc<Self>, kind: &'static str, work: F) -> Arc<Task>
    where
        F: FnOnce(Arc<Task>) -> Fut,
        Fut: std::future::Future<Output = anyhow::Result<T>> + Send + 'static,
        T: Serialize + Send + 'static,
    {
        let task = Arc::new(Task {
            id: uuid::Uuid::new_v4().to_string(),
            state: Mutex::new(Snapshot {
                id: String::new(),
                kind,
                status: Status::Running,
                progress: Progress { phase: "启动中".into(), done: 0, total: 0 },
                error: None,
                result: None,
            }),
            cancel_requested: AtomicBool::new(false),
            cancel_notify: Notify::new(),
        });
        task.state.lock().expect("task state poisoned").id = task.id.clone();
        self.insert(task.clone());

        let fut = work(task.clone());
        let handle = task.clone();
        tokio::spawn(async move {
            match fut.await {
                Ok(value) => {
                    let status = if handle.is_cancelled() { Status::Cancelled } else { Status::Done };
                    handle.finish(status, None, serde_json::to_value(value).ok());
                }
                Err(e) => {
                    let status = if handle.is_cancelled() { Status::Cancelled } else { Status::Failed };
                    handle.finish(status, Some(format!("{e:#}")), None);
                }
            }
        });
        task
    }

    pub fn get(&self, id: &str) -> Option<Arc<Task>> {
        self.tasks.lock().expect("task list poisoned").iter().find(|t| t.id == id).cloned()
    }

    /// Ask a running task to stop at its next checkpoint. Returns false if it is unknown.
    pub fn cancel(&self, id: &str) -> bool {
        let Some(task) = self.get(id) else { return false };
        task.request_cancel();
        true
    }

    /// Ask every running task to stop at its next checkpoint.
    pub fn cancel_all(&self) {
        self.running().iter().for_each(|t| t.request_cancel());
    }

    /// Cancel every running task, then wait up to `grace` for them to finish.
    /// Returns false if some were still running when the time ran out.
    pub async fn shut_down(&self, grace: Duration) -> bool {
        self.cancel_all();
        let running = self.running();
        let all_finished = async {
            while running.iter().any(|t| t.snapshot().status == Status::Running) {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        };
        tokio::time::timeout(grace, all_finished).await.is_ok()
    }

    fn running(&self) -> Vec<Arc<Task>> {
        let tasks = self.tasks.lock().expect("task list poisoned");
        tasks.iter().filter(|t| t.snapshot().status == Status::Running).cloned().collect()
    }

    fn insert(&self, task: Arc<Task>) {
        let mut tasks = self.tasks.lock().expect("task list poisoned");
        tasks.push_back(task);
        let finished = tasks.iter().filter(|t| t.snapshot().status != Status::Running).count();
        if finished > KEEP_FINISHED {
            if let Some(pos) = tasks.iter().position(|t| t.snapshot().status != Status::Running) {
                tasks.remove(pos);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn wait_until_finished(task: &Task) -> Snapshot {
        for _ in 0..100 {
            let s = task.snapshot();
            if s.status != Status::Running {
                return s;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("task did not finish");
    }

    #[tokio::test]
    async fn cancelling_interrupts_a_long_wait_and_keeps_the_partial_result() {
        let registry = Arc::new(TaskRegistry::default());
        let task = registry.spawn("test", |task| async move {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(60)) => {}
                _ = task.cancelled() => {}
            }
            Ok("partial")
        });
        assert!(registry.cancel(&task.id));
        let snap = wait_until_finished(&task).await;
        assert_eq!(snap.status, Status::Cancelled);
        assert_eq!(snap.result, Some(serde_json::json!("partial")));
    }

    #[tokio::test]
    async fn shutting_down_cancels_running_tasks_and_waits_for_them() {
        let registry = Arc::new(TaskRegistry::default());
        let task = registry.spawn("test", |task| async move {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(60)) => {}
                _ = task.cancelled() => {}
            }
            Ok(())
        });
        assert!(registry.shut_down(Duration::from_secs(5)).await);
        assert_eq!(task.snapshot().status, Status::Cancelled);
    }

    #[tokio::test]
    async fn shutting_down_gives_up_on_a_task_that_ignores_cancellation() {
        let registry = Arc::new(TaskRegistry::default());
        registry.spawn("test", |_| async {
            tokio::time::sleep(Duration::from_secs(60)).await;
            Ok(())
        });
        assert!(!registry.shut_down(Duration::from_millis(100)).await);
    }

    #[tokio::test]
    async fn an_error_fails_the_task_with_its_message() {
        let registry = Arc::new(TaskRegistry::default());
        let task = registry.spawn("test", |_| async { anyhow::Result::<()>::Err(anyhow::anyhow!("boom")) });
        let snap = wait_until_finished(&task).await;
        assert_eq!(snap.status, Status::Failed);
        assert_eq!(snap.error.as_deref(), Some("boom"));
    }
}
