use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::fmt::Debug;
use tokio::sync::Mutex;
use uuid::Uuid;

// Enum representing the status of a task
#[derive(Debug, Clone, PartialEq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed {
        result: Option<String>,
        duration: Duration,
    },
    Failed {
        error: String,
        duration: Duration,
    },
}

// Struct containing information about a task
#[derive(Debug)]
struct TaskInfo {
    id: String,
    name: String,
    status: TaskStatus,
    created_at: Instant,
}

// TaskManager struct to manage tasks
pub struct TaskManager {
    tasks: Arc<Mutex<HashMap<String, TaskInfo>>>,
}

impl TaskManager {
    // Creates a new TaskManager
    pub fn new() -> Self {
        TaskManager {
            tasks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    // Submits a new task to the TaskManager
    pub fn submit<F, Fut, T, E>(&self, name: &str, f: F) -> String
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, E>> + Send + 'static,
        T: Send + 'static,
        E: Debug + Send + 'static,
    {
        // Generate a unique ID for the task
        let task_id = Uuid::new_v4().to_string();
        let task_info = TaskInfo {
            id: task_id.clone(),
            name: name.to_string(),
            status: TaskStatus::Pending,
            created_at: Instant::now(),
        };

        // Insert the task into the task list
        {
            let mut tasks = self.tasks.blocking_lock();
            tasks.insert(task_id.clone(), task_info);
        }

        let tasks = Arc::clone(&self.tasks);
        let task_id_clone = task_id.clone();

        // Spawn a new asynchronous task
        tokio::spawn(async move {
            {
                let mut tasks = tasks.lock().await;
                if let Some(task) = tasks.get_mut(&task_id_clone) {
                    task.status = TaskStatus::Running;
                }
            }

            let start_time = Instant::now();
            let result = f().await;
            let duration = start_time.elapsed();

            let mut tasks = tasks.lock().await;
            if let Some(task) = tasks.get_mut(&task_id_clone) {
                task.status = match result {
                    Ok(_) => TaskStatus::Completed {
                        result: None,
                        duration,
                    },
                    Err(e) => TaskStatus::Failed {
                        error: format!("{:?}", e),
                        duration,
                    },
                };
            }
        });

        task_id
    }

    // Retrieves the status of a task by its ID
    pub async fn get_task_status(&self, task_id: &str) -> Option<TaskStatus> {
        let tasks = self.tasks.lock().await;
        tasks.get(task_id).map(|task| task.status.clone())
    }

    // Retrieves all tasks with their ID, name, and status
    pub async fn get_all_tasks(&self) -> Vec<(String, String, TaskStatus)> {
        let tasks = self.tasks.lock().await;
        tasks
            .iter()
            .map(|(id, task)| (id.clone(), task.name.clone(), task.status.clone()))
            .collect()
    }
}
