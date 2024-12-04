use crate::protocol::{Request, RequestKind, Response, ResponseKind};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::error::SendError;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tracing::{info, trace};
use uuid::Uuid;

#[cfg(test)]
mod tests;

pub mod daemon;
pub(crate) mod player;

pub mod protocol;
pub mod provider;
pub mod source;

mod utils;

pub fn init_logger() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .try_init();
}

pub struct TaskPool {
    tasks: Mutex<HashMap<String, oneshot::Sender<ResponseKind>>>,
    task_sender: UnboundedSender<Request>,
}

impl TaskPool {
    pub fn new(task_sender: UnboundedSender<Request>) -> Arc<Self> {
        Arc::new(Self {
            tasks: Mutex::new(HashMap::new()),
            task_sender,
        })
    }

    pub fn new_task(
        self: Arc<Self>,
        request_kind: RequestKind,
    ) -> Result<oneshot::Receiver<ResponseKind>, SendError<Request>> {
        let id = Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        let request = Request::new(request_kind, id.clone());
        trace!("Created new task: {}", id);

        trace!("Sending task to daemon");
        self.task_sender.send(request)?;

        trace!("Adding task to waiting list");
        self.tasks.lock().unwrap().insert(id, tx);
        Ok(rx)
    }

    pub fn main_loop(self: Arc<Self>, mut result_receiver: UnboundedReceiver<Response>) {
        info!("Starting main loop");
        let this = self.clone();

        tokio::spawn(async move {
            while let Some(response) = result_receiver.recv().await {
                let id = response.id.clone();
                trace!("Received response from daemon: {:?}", id);

                if let Some(sender) = this.tasks.lock().unwrap().remove(&id) {
                    trace!("Sending response to requester");
                    let _ = sender.send(response.kind);
                }
            }
        });
    }
}
