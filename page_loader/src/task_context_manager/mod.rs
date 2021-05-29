use std::sync::Arc;

use crate::task_context::TaskContext;

trait TaskContextManger {
    fn add_task(&mut self, task: Arc<dyn TaskContext>);
    fn start_garbage_collection_thread(&mut self);
}

#[cfg(test)]
mod tests {
    use std::fmt::{Debug, Formatter, Result};
    use std::sync::Arc;

    use mockall::*;
    use tokio::time::Instant;
    use uuid::Uuid;

    use crate::task_context::{TaskConfig, TaskContext};

    use super::*;

    mock! {
        MyTaskContext {}
        impl TaskContext for MyTaskContext {
            fn get_uuid_clone(&self) -> Uuid;
            fn get_config_clone(&self) -> TaskConfig;
            fn get_last_load_page_command_received_instant(&self) -> Option<Instant>;
            fn can_be_garbage_collected(&self) -> bool;
        }

        impl Debug for MyTaskContext {
            fn fmt<'a>(&self, f: &mut Formatter<'a>) -> Result;
        }
    }

    #[tokio::test]
    async fn added_task_context_gets_garbage_collected_after_timeout() {
        // given
        let mut mock_task_context = MockMyTaskContext::new();
        mock_task_context.expect_can_be_garbage_collected().returning(|| true);
        let task_context = Arc::new(mock_task_context);

        // when
        todo!("Continue implementing")

        //then
    }
}