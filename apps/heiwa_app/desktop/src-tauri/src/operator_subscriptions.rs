//! Window-scoped observation leases. Dropping a lease closes the websocket;
//! it never cancels the durable operator turn being observed.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::watch;

const MAX_LEASES: usize = 256;
const EARLY_CANCEL_TTL: Duration = Duration::from_secs(60);
type Key = (String, String);

enum Entry {
    Active(watch::Sender<bool>),
    CancelledBeforeStart(Instant),
}

#[derive(Clone, Default)]
pub struct OperatorSubscriptions(Arc<Mutex<HashMap<Key, Entry>>>);

pub struct ObservationLease {
    registry: OperatorSubscriptions,
    key: Key,
    pub cancelled: watch::Receiver<bool>,
}

fn key(window: &str, id: &str) -> Result<Key, String> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err("invalid subscription ID".into());
    }
    Ok((window.into(), id.into()))
}

fn prune(entries: &mut HashMap<Key, Entry>) {
    entries.retain(|_, entry| match entry {
        Entry::Active(_) => true,
        Entry::CancelledBeforeStart(at) => at.elapsed() < EARLY_CANCEL_TTL,
    });
}

impl OperatorSubscriptions {
    pub fn register(&self, window: &str, id: &str) -> Result<ObservationLease, String> {
        let key = key(window, id)?;
        let mut entries = self
            .0
            .lock()
            .map_err(|_| "subscription registry unavailable")?;
        prune(&mut entries);
        if matches!(entries.get(&key), Some(Entry::Active(_))) {
            return Err("subscription ID already in use".into());
        }
        let cancelled = matches!(entries.get(&key), Some(Entry::CancelledBeforeStart(_)));
        if !entries.contains_key(&key) && entries.len() >= MAX_LEASES {
            return Err("too many active subscriptions".into());
        }
        let (sender, receiver) = watch::channel(cancelled);
        entries.insert(key.clone(), Entry::Active(sender));
        Ok(ObservationLease {
            registry: self.clone(),
            key,
            cancelled: receiver,
        })
    }

    pub fn cancel(&self, window: &str, id: &str) -> Result<(), String> {
        let key = key(window, id)?;
        let mut entries = self
            .0
            .lock()
            .map_err(|_| "subscription registry unavailable")?;
        prune(&mut entries);
        match entries.get(&key) {
            Some(Entry::Active(sender)) => {
                sender.send_replace(true);
            }
            Some(Entry::CancelledBeforeStart(_)) => {}
            None => {
                if entries.len() >= MAX_LEASES {
                    return Err("too many active subscriptions".into());
                }
                // IPC delivery may put cancellation ahead of registration.
                entries.insert(key, Entry::CancelledBeforeStart(Instant::now()));
            }
        }
        Ok(())
    }

    pub fn cancel_window(&self, window: &str) {
        if let Ok(mut entries) = self.0.lock() {
            entries.retain(|(owner, _), entry| {
                if owner != window {
                    return true;
                }
                if let Entry::Active(sender) = entry {
                    sender.send_replace(true);
                }
                false
            });
        }
    }
}

impl Drop for ObservationLease {
    fn drop(&mut self) {
        if let Ok(mut entries) = self.registry.0.lock() {
            // A closed window can remove the old lease before its future drops.
            // Do not remove a replacement with the same key from a later window.
            let owns_entry = matches!(entries.get(&self.key), Some(Entry::Active(sender)) if sender.subscribe().same_channel(&self.cancelled));
            if owns_entry {
                entries.remove(&self.key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_is_scoped_and_releases_on_drop() {
        let registry = OperatorSubscriptions::default();
        let a = registry.register("a", "stream").unwrap();
        let b = registry.register("b", "stream").unwrap();
        registry.cancel("a", "stream").unwrap();
        assert!(*a.cancelled.borrow());
        assert!(!*b.cancelled.borrow());
        assert!(registry.register("a", "stream").is_err());
        drop(a);
        assert!(!*registry.register("a", "stream").unwrap().cancelled.borrow());
    }

    #[test]
    fn early_cancellation_is_honored_and_bounded() {
        let registry = OperatorSubscriptions::default();
        registry.cancel("a", "first").unwrap();
        assert!(*registry.register("a", "first").unwrap().cancelled.borrow());
        for i in 0..MAX_LEASES {
            registry.cancel("a", &format!("s-{i}")).unwrap();
        }
        assert!(registry.cancel("a", "overflow").is_err());
        registry.cancel_window("a");
        assert!(registry.register("a", "overflow").is_ok());
    }

    #[test]
    fn closing_window_cancels_only_its_observers() {
        let registry = OperatorSubscriptions::default();
        let a = registry.register("a", "stream").unwrap();
        let b = registry.register("b", "stream").unwrap();
        registry.cancel_window("a");
        assert!(*a.cancelled.borrow());
        assert!(!*b.cancelled.borrow());
        let replacement = registry.register("a", "stream").unwrap();
        drop(a);
        registry.cancel("a", "stream").unwrap();
        assert!(*replacement.cancelled.borrow());
    }
}
