use crate::{
    Clipboard, Clock, DesktopShell, Enrollment, Screenshot, UiController, UiError, UiIntent,
    ViewModel,
};
use std::sync::mpsc::{channel, Receiver, Sender};

pub trait Controller: Send {
    fn dispatch(&mut self, intent: UiIntent) -> Result<(), UiError>;
    fn snapshot(&self) -> ViewModel;
}

impl<S, C, E, D, K> Controller for UiController<S, C, E, D, K>
where
    S: Screenshot + Send,
    C: Clipboard + Send,
    E: Enrollment + Send,
    D: DesktopShell + Send,
    K: Clock + Send,
{
    fn dispatch(&mut self, intent: UiIntent) -> Result<(), UiError> {
        UiController::dispatch(self, intent)
    }

    fn snapshot(&self) -> ViewModel {
        self.view().clone()
    }
}

#[derive(Debug, Clone)]
pub struct UiUpdate {
    pub view: ViewModel,
    pub error: Option<String>,
}

#[derive(Clone)]
pub struct UiIntentSender(Sender<UiIntent>);

impl UiIntentSender {
    pub fn send(&self, intent: UiIntent) -> Result<(), UiError> {
        self.0
            .send(intent)
            .map_err(|_| UiError::InvalidState("UI worker 已停止".to_owned()))
    }
}

pub struct UiWorker {
    sender: UiIntentSender,
    updates: Receiver<UiUpdate>,
}

impl UiWorker {
    pub fn spawn<C: Controller + 'static>(mut controller: C) -> Self {
        let (intent_sender, intent_receiver) = channel::<UiIntent>();
        let (update_sender, update_receiver) = channel::<UiUpdate>();
        std::thread::spawn(move || {
            if update_sender
                .send(UiUpdate {
                    view: controller.snapshot(),
                    error: None,
                })
                .is_err()
            {
                return;
            }
            while let Ok(intent) = intent_receiver.recv() {
                let result = controller.dispatch(intent);
                let update = UiUpdate {
                    view: controller.snapshot(),
                    error: result.err().map(|error| error.to_string()),
                };
                if update_sender.send(update).is_err() {
                    break;
                }
            }
        });
        Self {
            sender: UiIntentSender(intent_sender),
            updates: update_receiver,
        }
    }

    pub fn into_parts(self) -> (UiIntentSender, Receiver<UiUpdate>) {
        (self.sender, self.updates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    struct FakeController {
        view: ViewModel,
    }

    impl Controller for FakeController {
        fn dispatch(&mut self, _intent: UiIntent) -> Result<(), UiError> {
            self.view.capture.message = Some(format!("{:?}", std::thread::current().id()));
            Ok(())
        }

        fn snapshot(&self) -> ViewModel {
            self.view.clone()
        }
    }

    #[test]
    fn controller_work_runs_off_the_calling_thread() {
        let worker = UiWorker::spawn(FakeController {
            view: ViewModel::default(),
        });
        let (sender, updates) = worker.into_parts();
        updates.recv_timeout(Duration::from_secs(1)).unwrap();
        sender.send(UiIntent::Refresh).unwrap();
        let update = updates.recv_timeout(Duration::from_secs(1)).unwrap();
        let worker_thread = update.view.capture.message.unwrap();
        assert_ne!(worker_thread, format!("{:?}", std::thread::current().id()));
    }
}
