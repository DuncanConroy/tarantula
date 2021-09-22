use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use log::{debug, error};

use crate::events::crawler_event::CrawlerEvent;
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
        let key = task.lock().unwrap().get_uuid_clone().to_string();
        debug!("Strong pointers to task {} before insert: {}", &key, Arc::strong_count(&task));
        self.tasks.lock().unwrap().insert(key.clone(), task);
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
            .spawn(move || DefaultTaskManager::run(cloned_manager, Duration::from_millis(gc_timeout_ms as u64)))
            .unwrap();

        manager
    }

    fn get_number_of_tasks(&self) -> usize {
        self.tasks.lock().unwrap().len()
    }
}

impl DefaultTaskManager {
    fn run(manager_instance: Arc<Mutex<DefaultTaskManager>>, gc_timeout_ms: Duration) {
        loop {
            thread::sleep(gc_timeout_ms);

            manager_instance.lock().unwrap().do_garbage_collection();
        }
    }

    fn do_garbage_collection(&mut self) {
        let mut to_gc = vec![];
        for (key, value) in self.tasks.lock().unwrap().iter() {
            if value.lock().unwrap().can_be_garbage_collected(self.gc_timeout_ms) {
                let uuid = value.lock().unwrap().get_uuid_clone();
                if let Err(error) = value.lock().unwrap()
                    .get_response_channel()
                    .blocking_send(CrawlerEvent::CompleteEvent { uuid: uuid.clone() }) {
                    error!("Error while sending CompleteEvent to channel of task {}, error: {}", uuid, error);
                }
                to_gc.push(key.clone());
            }
        }

        to_gc.iter().for_each(|key|
            debug!("Strong pointers to task {} before gc: {}",
                key,
                Arc::strong_count(&self.tasks.lock().unwrap()[key])
            ));
        self.tasks.lock().unwrap().retain(|key, _| !to_gc.contains(key));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use mockall::*;
    use tokio::sync::mpsc;
    use tokio::sync::mpsc::Sender;
    use tokio::time::Duration;
    use tokio::time::Instant;
    use uuid::Uuid;

    use crate::events::crawler_event::CrawlerEvent;
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
            fn get_response_channel(&self) -> &Sender<CrawlerEvent>;
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn added_task_context_gets_garbage_collected_after_timeout() {
        // given
        let (resp_tx, mut resp_rx) = mpsc::channel(2);
        let expected_uuid = Uuid::new_v4();
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_can_be_garbage_collected().returning(|#[allow(unused_variables)] // allowing, as we don't use gc_timeout_ms
                                                                       gc_timeout_ms: u64| true);
        mock_task_context.expect_get_url().returning(|| String::from("https://example.com"));
        mock_task_context.expect_get_response_channel().return_const(resp_tx);
        mock_task_context.expect_get_uuid_clone().return_const(expected_uuid);
        let task_context = Arc::new(Mutex::new(mock_task_context));
        let gc_timeout_ms = 100u64;
        let task_manager = DefaultTaskManager::init(gc_timeout_ms);

        tokio::spawn(async move {
            // when
            task_manager.lock().unwrap().add_task(task_context);

            //then
            let num_tasks = task_manager.lock().unwrap().get_number_of_tasks();
            assert_eq!(num_tasks, 1, "task was not added");
            tokio::time::sleep(Duration::from_millis(gc_timeout_ms as u64 * 2)).await;
            if let CrawlerEvent::CompleteEvent { uuid: actual_uuid } = resp_rx.recv().await.unwrap() {
                assert_eq!(expected_uuid, actual_uuid);
            } else {
                panic!("No complete event received before garbage collection!");
            }
            let num_tasks = task_manager.lock().unwrap().get_number_of_tasks();
            assert_eq!(num_tasks, 0, "task was not removed");
        }).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn added_task_context_does_not_get_garbage_collected_within_timeout() {
        // given
        let mut mock_task_context = MockMyTaskContext::new();
        let expected_uuid = Uuid::new_v4();
        mock_task_context.expect_can_be_garbage_collected().returning(|#[allow(unused_variables)] // allowing dead code, as we don't use gc_timeout_ms
                                                                       gc_timeout_ms: u64| false);
        mock_task_context.expect_get_url().returning(|| String::from("https://example.com"));
        mock_task_context.expect_get_uuid_clone().return_const(expected_uuid);

        let task_context = Arc::new(Mutex::new(mock_task_context));
        let gc_timeout_ms = 100u64;
        let task_manager = DefaultTaskManager::init(gc_timeout_ms);

        tokio::spawn(async move {
            // when
            task_manager.lock().unwrap().add_task(task_context);

            //then
            let num_tasks = task_manager.lock().unwrap().get_number_of_tasks();
            assert_eq!(num_tasks, 1, "task was not added");
            tokio::time::sleep(Duration::from_millis(gc_timeout_ms as u64 * 2)).await;
            let num_tasks = task_manager.lock().unwrap().get_number_of_tasks();
            assert_eq!(num_tasks, 1, "task was not removed");
        }).await.unwrap();
    }
}