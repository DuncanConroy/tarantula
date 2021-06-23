use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::task_context::task_context::TaskContext;

pub trait TaskManager: Sync + Send {
    fn add_task(&mut self, task: Arc<Mutex<dyn TaskContext>>);
    fn init(gc_timeout_ms: u64) -> Arc<Mutex<Self>> where Self: Sized;
    fn get_number_of_tasks(&self) -> usize;
}

pub struct DefaultTaskManager {
    // URL - TaskContext
    tasks: Arc<Mutex<HashMap<String, Arc<Mutex<dyn TaskContext>>>>>,
    // garbage collection timeout in seconds
    gc_timeout_ms: u64,
}

impl TaskManager for DefaultTaskManager {
    fn add_task(&mut self, task: Arc<Mutex<dyn TaskContext>>) {
        let task_clone = task.clone();
        self.tasks.lock().unwrap().insert(task_clone.lock().unwrap().get_url(), task_clone.clone());
    }

    fn init(gc_timeout_ms: u64) -> Arc<Mutex<Self>> {
        let manager = Arc::new(Mutex::new(
            DefaultTaskManager {
                tasks: Arc::new(Mutex::new(HashMap::new())),
                gc_timeout_ms,
            }));

        let cloned_manager = manager.clone();
        thread::Builder::new()
            .name("DefaultTaskManager garbage collection".to_owned())
            .spawn(move || DefaultTaskManager::run(cloned_manager, Duration::from_secs(gc_timeout_ms as u64)))
            .unwrap();

        manager
    }

    fn get_number_of_tasks(&self) -> usize {
        self.tasks.lock().unwrap().len()
    }
}

impl DefaultTaskManager {
    fn run(manager_instance: Arc<Mutex<DefaultTaskManager>>, mut gc_timeout_ms: Duration) {
        loop {
            thread::sleep(gc_timeout_ms);

            manager_instance.lock().unwrap().do_garbage_collection();
        }
    }

    fn do_garbage_collection(&mut self) {
        self.tasks.lock().unwrap().retain(|_, value| { !value.lock().unwrap().can_be_garbage_collected(self.gc_timeout_ms) })
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::{Debug, Formatter, Result};
    use std::sync::Arc;

    use mockall::*;
    use tokio::time::Duration;
    use tokio::time::Instant;
    use uuid::Uuid;

    use crate::task_context::task_context::{TaskConfig, TaskContext};

    use super::*;

    mock! {
        MyTaskContext {}
        impl TaskContext for MyTaskContext {
            fn get_uuid_clone(&self) -> Uuid;
            fn get_config(&self) -> Arc<Mutex<TaskConfig>>;
            fn get_url(&self)->String;
            fn get_last_command_received(&self) -> Instant;
            fn set_last_command_received(&mut self, instant: Instant);
            fn can_be_garbage_collected(&self, gc_timeout_ms: u64)-> bool;
        }

        impl Debug for MyTaskContext {
            fn fmt<'a>(&self, f: &mut Formatter<'a>) -> Result;
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn added_task_context_gets_garbage_collected_after_timeout() {
        // given
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_can_be_garbage_collected().returning(|gc_timeout_ms: u64| true);
        mock_task_context.expect_get_url().returning(|| String::from("https://example.com"));
        let task_context = Arc::new(Mutex::new(mock_task_context));
        let gc_timeout_ms = 1u64;
        let task_manager = DefaultTaskManager::init(gc_timeout_ms);

        tokio::spawn(async move {
            // when
            task_manager.lock().unwrap().add_task(task_context);

            //then
            let num_tasks = task_manager.lock().unwrap().get_number_of_tasks();
            assert_eq!(num_tasks, 1, "task was not added");
            tokio::time::sleep(Duration::from_secs(gc_timeout_ms as u64 * 2)).await;
            let num_tasks = task_manager.lock().unwrap().get_number_of_tasks();
            assert_eq!(num_tasks, 0, "task was not removed");
        }).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn added_task_context_does_not_get_garbage_collected_within_timeout() {
        // given
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_can_be_garbage_collected().returning(|gc_timeout_ms: u64| false);
        mock_task_context.expect_get_url().returning(|| String::from("https://example.com"));
        let task_context = Arc::new(Mutex::new(mock_task_context));
        let gc_timeout_ms = 1u64;
        let task_manager = DefaultTaskManager::init(gc_timeout_ms);

        tokio::spawn(async move {
            // when
            task_manager.lock().unwrap().add_task(task_context);

            //then
            let num_tasks = task_manager.lock().unwrap().get_number_of_tasks();
            assert_eq!(num_tasks, 1, "task was not added");
            tokio::time::sleep(Duration::from_secs(gc_timeout_ms as u64 * 2)).await;
            let num_tasks = task_manager.lock().unwrap().get_number_of_tasks();
            assert_eq!(num_tasks, 1, "task was not removed");
        }).await.unwrap();
    }
}