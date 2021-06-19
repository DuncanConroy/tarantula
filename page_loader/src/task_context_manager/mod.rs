use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::task_context::task_context::TaskContext;

pub trait TaskManager: Sync + Send {
    fn add_task(&mut self, task: Arc<dyn TaskContext>);
    fn init(gc_timeout: u16) -> Arc<Mutex<Self>> where Self: Sized;
    fn get_number_of_tasks(&self) -> usize;
}

pub struct DefaultTaskManager {
    // URL - TaskContext
    tasks: Arc<Mutex<HashMap<String, Arc<dyn TaskContext>>>>,
    // garbage collection timeout in seconds
    gc_timeout: u16,
}

impl TaskManager for DefaultTaskManager {
    fn add_task(&mut self, task: Arc<dyn TaskContext>) {
        self.tasks.lock().unwrap().insert(task.get_url(), task);
    }

    fn init(gc_timeout: u16) -> Arc<Mutex<Self>> {
        let manager = Arc::new(Mutex::new(
            DefaultTaskManager {
                tasks: Arc::new(Mutex::new(HashMap::new())),
                gc_timeout,
            }));

        let cloned_manager = manager.clone();
        thread::Builder::new()
            .name("DefaultTaskManager garbage collection".to_owned())
            .spawn(move || DefaultTaskManager::run(cloned_manager, Duration::from_secs(gc_timeout as u64)))
            .unwrap();

        manager
    }

    fn get_number_of_tasks(&self) -> usize {
        self.tasks.lock().unwrap().len()
    }
}

impl DefaultTaskManager {
    fn run(manager_instance: Arc<Mutex<DefaultTaskManager>>, mut gc_timeout: Duration) {
        loop {
            thread::sleep(gc_timeout);

            manager_instance.lock().unwrap().do_garbage_collection();
        }
    }

    fn do_garbage_collection(&mut self) {
        self.tasks.lock().unwrap().retain(|_, value| { !value.can_be_garbage_collected() })
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
            fn get_last_load_page_command_received_instant(&self) -> Option<Instant>;
            fn can_be_garbage_collected(&self) -> bool;
        }

        impl Debug for MyTaskContext {
            fn fmt<'a>(&self, f: &mut Formatter<'a>) -> Result;
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn added_task_context_gets_garbage_collected_after_timeout() {
        // given
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_can_be_garbage_collected().returning(|| true);
        mock_task_context.expect_get_url().returning(|| String::from("https://example.com"));
        let task_context = Arc::new(mock_task_context);
        let gc_timeout = 1u16;
        let task_manager = DefaultTaskManager::init(gc_timeout);

        tokio::spawn(async move {
            // when
            task_manager.lock().unwrap().add_task(task_context);

            //then
            let num_tasks = task_manager.lock().unwrap().get_number_of_tasks();
            assert_eq!(num_tasks, 1, "task was not added");
            tokio::time::sleep(Duration::from_secs(gc_timeout as u64 * 2)).await;
            let num_tasks = task_manager.lock().unwrap().get_number_of_tasks();
            assert_eq!(num_tasks, 0, "task was not removed");
        }).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn added_task_context_does_not_get_garbage_collected_within_timeout() {
        // given
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_can_be_garbage_collected().returning(|| false);
        mock_task_context.expect_get_url().returning(|| String::from("https://example.com"));
        let task_context = Arc::new(mock_task_context);
        let gc_timeout = 1u16;
        let task_manager = DefaultTaskManager::init(gc_timeout);

        tokio::spawn(async move {
            // when
            task_manager.lock().unwrap().add_task(task_context);

            //then
            let num_tasks = task_manager.lock().unwrap().get_number_of_tasks();
            assert_eq!(num_tasks, 1, "task was not added");
            tokio::time::sleep(Duration::from_secs(gc_timeout as u64 * 2)).await;
            let num_tasks = task_manager.lock().unwrap().get_number_of_tasks();
            assert_eq!(num_tasks, 1, "task was not removed");
        }).await.unwrap();
    }
}