use actix::{dev::MessageResponse, Actor, AsyncContext, Context, Message};
use std::{
    any::TypeId,
    collections::{HashMap, VecDeque},
    fmt::Debug,
    future::Future,
    time::Duration,
};
use tokio::task::JoinHandle;
use tracing::{debug, trace};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum TaskError {}

#[derive(Message)]
#[rtype(result = "TaskStatus<T>")]
pub enum TaskMessage<T: Send + 'static> {
    Sync(Box<dyn FnOnce() -> T + Send>),
    Async(Box<dyn Future<Output = T> + Send>),
}

impl<T: Send > Debug for TaskMessage<T> {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskMessage::Sync(_) => write!(fmt, "TaskMessage::Sync({:?})", TypeId::of::<T>()),
            TaskMessage::Async(_) => write!(fmt, "TaskMessage::ASync({:?})", TypeId::of::<T>()),
        }
    }
}

#[derive(Debug)]
pub enum TaskStatus<T> {
    Pending,
    Running(JoinHandle<T>),
    Finished(T),
}

impl<A, M, T> MessageResponse<A, M> for TaskStatus<T>
where
    A: Actor,
    M: Message<Result = Self>,
{
    fn handle(self, _ctx: &mut A::Context, tx: Option<actix::dev::OneshotSender<M::Result>>) {
        if let Some(tx) = tx {
            let _ = tx.send(self);
        }
    }
}

pub struct TaskManager<T: Send + 'static> {
    task_queue: VecDeque<Uuid>,
    task_registry: HashMap<Uuid, TaskMessage<T>>,
    max_concurrent_tasks: usize,
    current_tasks: usize,
    runtime: tokio::runtime::Runtime,
}

impl<T: Send + 'static> TaskManager<T> {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            task_queue: VecDeque::new(),
            task_registry: HashMap::new(),
            max_concurrent_tasks: max_concurrent,
            current_tasks: 0,
            runtime: tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap(),
        }
    }

    fn execute_task(&mut self, task: TaskMessage<T>) -> JoinHandle<T> {
        trace!("Executing task {:?}", task);
        match task {
            TaskMessage::Sync(f) => self.runtime.spawn_blocking(f),
            TaskMessage::Async(f) => self.runtime.spawn(Box::into_pin(f)),
        }
    }

    fn process_next_task(&mut self, ctx: &mut Context<Self>) {
        if self.current_tasks < self.max_concurrent_tasks {
            if let Some(task) = self.task_queue.pop_front() {
                let task = self.task_registry.remove(&task).unwrap();
                // Schedule the task execution
                ctx.notify_later(task, Duration::from_millis(0));
            }
        }
    }
}

impl<T: Send + 'static> Actor for TaskManager<T> {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        debug!("TaskManager is started");
        self.process_next_task(ctx);
    }

    fn stopped(&mut self, _ctx: &mut Context<Self>) {
        debug!("TaskManager is stopped");
    }
}

impl<T: Send + 'static> actix::Handler<TaskMessage<T>> for TaskManager<T> {
    type Result = TaskStatus<T>;

    fn handle(&mut self, msg: TaskMessage<T>, ctx: &mut Self::Context) -> Self::Result {
        trace!("Executing task {:?}", msg);

        if self.current_tasks >= self.max_concurrent_tasks {
            let uuid = Uuid::new_v4();
            self.task_queue.push_back(uuid);
            self.task_registry.insert(uuid, msg);
            return TaskStatus::Pending;
        }

        let handle = self.execute_task(msg);

        let result = if handle.is_finished() {
            let result = self.runtime.block_on(handle).unwrap();
            TaskStatus::Finished(result)
        } else {
            self.current_tasks += 1;
            TaskStatus::Running(handle)
        };

        // Try notifying the next task
        self.process_next_task(ctx);

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init_logger;
    use std::sync::{Arc, Mutex};

    #[actix::test]
    async fn test_sync() {
        init_logger();

        let addr = TaskManager::new(16).start();
        let expected_list = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
        let actual_list = Arc::new(Mutex::new(Vec::new()));

        for i in 0..10 {
            let list = actual_list.clone();
            let task = TaskMessage::Sync(Box::new(move || {
                std::thread::sleep(Duration::from_millis(i * 100));
                list.lock().unwrap().push(i);
            }));

            addr.do_send(task);
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
        assert_eq!(*actual_list.lock().unwrap(), expected_list);
    }

    #[actix::test]
    async fn test_async() {
        init_logger();

        let addr = TaskManager::new(16).start();
        let expected_list = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
        let actual_list = Arc::new(Mutex::new(Vec::new()));

        for i in 0..10 {
            let list = actual_list.clone();
            let task = TaskMessage::Async(Box::new(async_sleep(i, list)));

            addr.do_send(task);
        }

        tokio::time::sleep(Duration::from_secs(2)).await;
        assert_eq!(*actual_list.lock().unwrap(), expected_list);
    }

    async fn async_sleep(sec: u64, list: Arc<Mutex<Vec<u64>>>) {
        tokio::time::sleep(Duration::from_millis(100 * sec)).await;
        list.lock().unwrap().push(sec);
    }
}
