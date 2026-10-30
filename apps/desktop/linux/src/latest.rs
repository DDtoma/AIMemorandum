use std::sync::{Condvar, Mutex};

struct State<T> {
    value: Option<T>,
    closed: bool,
}

pub struct Latest<T> {
    state: Mutex<State<T>>,
    wake: Condvar,
}

impl<T> Latest<T> {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State {
                value: None,
                closed: false,
            }),
            wake: Condvar::new(),
        }
    }

    pub fn publish(&self, value: T) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.closed {
            return;
        }
        state.value = Some(value);
        drop(state);
        self.wake.notify_one();
    }

    pub fn wait(&self) -> Option<T> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            if let Some(value) = state.value.take() {
                return Some(value);
            }
            if state.closed {
                return None;
            }
            state = self
                .wake
                .wait(state)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    pub fn close(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.closed = true;
        state.value = None;
        drop(state);
        self.wake.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::Latest;

    #[test]
    fn one_pending_slot_is_replaced_by_the_newest_value() {
        let latest = Latest::new();
        latest.publish(1);
        latest.publish(2);
        latest.publish(3);
        assert_eq!(latest.wait(), Some(3));
        latest.close();
        assert_eq!(latest.wait(), None);
    }
}
