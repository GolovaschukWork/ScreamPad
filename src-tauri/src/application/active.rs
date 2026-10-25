//! Registry of files that belong to running imports, so reconciliation never removes them.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Default)]
struct State {
    staging: HashSet<String>,
    keys: HashSet<String>,
}

#[derive(Default)]
pub struct ActiveImports(Mutex<State>);

impl ActiveImports {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn state(&self) -> MutexGuard<'_, State> {
        // A poisoned lock only means another thread panicked while holding it; the sets stay consistent.
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Registers a staging file name before the file is created. The returned guard unregisters it.
    pub fn register_staging(self: &Arc<Self>, name: &str) -> ActiveGuard {
        self.state().staging.insert(name.to_owned());
        ActiveGuard {
            registry: Arc::clone(self),
            name: name.to_owned(),
            kind: Kind::Staging,
        }
    }

    /// Registers a storage key that is about to be published.
    pub fn register_key(self: &Arc<Self>, key: &str) -> ActiveGuard {
        self.state().keys.insert(key.to_owned());
        ActiveGuard {
            registry: Arc::clone(self),
            name: key.to_owned(),
            kind: Kind::Key,
        }
    }

    pub fn is_staging_active(&self, name: &str) -> bool {
        self.state().staging.contains(name)
    }

    pub fn is_key_active(&self, key: &str) -> bool {
        self.state().keys.contains(key)
    }
}

enum Kind {
    Staging,
    Key,
}

pub struct ActiveGuard {
    registry: Arc<ActiveImports>,
    name: String,
    kind: Kind,
}

impl Drop for ActiveGuard {
    fn drop(&mut self) {
        let mut state = self.registry.state();
        match self.kind {
            Kind::Staging => state.staging.remove(&self.name),
            Kind::Key => state.keys.remove(&self.name),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guards_register_and_unregister() {
        let active = ActiveImports::new();
        {
            let _staging = active.register_staging("a.part");
            let _key = active.register_key("ab/abc.wav");
            assert!(active.is_staging_active("a.part"));
            assert!(active.is_key_active("ab/abc.wav"));
            assert!(!active.is_staging_active("b.part"));
        }
        assert!(!active.is_staging_active("a.part"));
        assert!(!active.is_key_active("ab/abc.wav"));
    }
}
