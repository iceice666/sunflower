use parking_lot::Mutex;
use std::sync::mpsc::RecvError;
use std::sync::Arc;

#[derive(Debug)]
struct Inner<T> {
    item: Option<T>,
}

#[derive(Debug, Clone)]
pub struct Sender<T> {
    inner: Arc<Mutex<Inner<T>>>,
}

#[derive(Debug)]
pub struct Receiver<T> {
    inner: Arc<Mutex<Inner<T>>>,
}

pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(Mutex::new(Inner { item: None }));
    (
        Sender {
            inner: inner.clone(),
        },
        Receiver { inner },
    )
}

impl<T> Sender<T> {
    pub fn update(&self, value: T) {
        let mut guard = self.inner.lock();
        guard.item = Some(value);
    }
}

impl<T> Receiver<T> {
    pub fn latest(&self) -> Result<T, RecvError> {
        let mut guard = self.inner.lock();
        guard.item.take().ok_or(RecvError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_item_channel() {
        let (tx, rx) = channel();

        // Test successful send and receive
        tx.update(42);
        assert_eq!(rx.latest().unwrap(), 42);

        // Test receive when empty
        assert!(rx.latest().is_err());

        // Test send when full
        tx.update(1);
        tx.update(2);
        assert_eq!(rx.latest().unwrap(), 2);
    }
}
