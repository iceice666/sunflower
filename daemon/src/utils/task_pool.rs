use crate::protocol::{Request, RequestKind, Response, ResponseKind};
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;
use tracing::{debug, error, info, instrument, trace};
use uuid::Uuid;

#[derive(Debug)]
pub struct TaskPool {
    tasks: DashMap<String, oneshot::Sender<ResponseKind>>,
    task_sender: UnboundedSender<Request>,
}

impl TaskPool {
    /// Creates a new TaskPool with the given task sender
    #[must_use]
    pub fn new(task_sender: UnboundedSender<Request>) -> Arc<Self> {
        Arc::new(Self {
            tasks: DashMap::new(),
            task_sender,
        })
    }

    /// Creates a new task with the given request kind
    ///
    /// # Errors
    /// Returns `SendError` if the task cannot be sent to the daemon
    #[instrument(skip(self))]
    pub fn new_task(
        self: &Arc<Self>,
        request_kind: RequestKind,
    ) -> Result<oneshot::Receiver<ResponseKind>, SendError<Request>> {
        let id = Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        let request = Request::new(request_kind, id.clone());

        trace!("Created new task");

        // Send the task before inserting into the map to prevent race conditions
        self.task_sender.send(request)?;
        debug!("Sent task to daemon");

        self.tasks.insert(id, tx);
        trace!("Added task to waiting list");

        Ok(rx)
    }

    /// Starts the main processing loop.
    /// This will not block the current thread.
    ///
    /// # Panics
    /// Will panic if the receiver channel is closed unexpectedly
    #[instrument(skip(self, result_receiver))]
    pub fn run(self: Arc<Self>, mut result_receiver: UnboundedReceiver<Response>) {
        tokio::spawn(async move {
            info!("Starting main loop");

            while let Some(response) = result_receiver.recv().await {
                let id = response.id.clone();
                trace!(?id, "Received response from daemon");

                match self.tasks.remove(&id) {
                    Some((_, sender)) => {
                        if let Err(response_kind) = sender.send(response.kind) {
                            error!(?id, ?response_kind, "Failed to send response to requester");
                        }
                        debug!(?id, "Successfully processed response");
                    }
                    None => {
                        error!(?id, "Received response for unknown task");
                    }
                }
            }

            error!("Result receiver channel closed unexpectedly");
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_task_lifecycle() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let pool = TaskPool::new(tx);

        // Spawn the main loop
        let pool_clone = pool.clone();
        let (result_tx, result_rx) = mpsc::unbounded_channel();

        pool_clone.run(result_rx);

        // Create a new task
        let request_kind = RequestKind::AreYouAlive;
        let response_rx = pool.new_task(request_kind).unwrap();

        // Simulate daemon processing
        let request = rx.recv().await.unwrap();
        result_tx
            .send(Response::new(ResponseKind::Ok(None), request.id))
            .unwrap();

        // Verify response
        let response = response_rx.await.unwrap();
        assert!(matches!(response, ResponseKind::Ok(None)));
    }
}
