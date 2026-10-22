//! What the engine needs from the registry, so its logic can be tested against
//! an in-memory stand-in and run against HKEY_CURRENT_USER.

use platform::registry::{self as reg, RawValue, RegistryError};

/// Values under one hive, addressed by key path and value name.
pub trait Registry {
    fn read(&self, path: &str, name: &str) -> Result<Option<RawValue>, RegistryError>;
    /// Writes the value, creating missing keys on the way.
    fn write(&mut self, path: &str, name: &str, value: &RawValue) -> Result<(), RegistryError>;
    /// Deletes the value; one that is already gone is not an error.
    fn delete(&mut self, path: &str, name: &str) -> Result<(), RegistryError>;
    fn key_exists(&self, path: &str) -> Result<bool, RegistryError>;
    /// Deletes the key if it holds nothing and reports whether it is gone.
    fn remove_key_if_empty(&mut self, path: &str) -> Result<bool, RegistryError>;
}

/// HKEY_CURRENT_USER, the only hive LimeKit changes.
#[derive(Debug, Default, Clone, Copy)]
pub struct CurrentUser;

impl Registry for CurrentUser {
    fn read(&self, path: &str, name: &str) -> Result<Option<RawValue>, RegistryError> {
        reg::read_value(path, name)
    }

    fn write(&mut self, path: &str, name: &str, value: &RawValue) -> Result<(), RegistryError> {
        reg::write_value(path, name, value)
    }

    fn delete(&mut self, path: &str, name: &str) -> Result<(), RegistryError> {
        reg::delete_value(path, name)
    }

    fn key_exists(&self, path: &str) -> Result<bool, RegistryError> {
        reg::key_exists(path)
    }

    fn remove_key_if_empty(&mut self, path: &str) -> Result<bool, RegistryError> {
        reg::remove_key_if_empty(path)
    }
}

#[cfg(test)]
pub(crate) mod fake {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;

    /// An in-memory hive. Paths and names are case-insensitive, as in the
    /// registry. It can also refuse writes or swallow them, the two ways
    /// Windows turns a change down.
    #[derive(Debug, Default)]
    pub struct FakeRegistry {
        values: BTreeMap<(String, String), RawValue>,
        keys: BTreeSet<String>,
        denied: BTreeSet<String>,
        ignored: BTreeSet<String>,
    }

    fn fold(text: &str) -> String {
        text.to_lowercase()
    }

    impl FakeRegistry {
        pub fn with(mut self, path: &str, name: &str, value: RawValue) -> Self {
            self.write(path, name, &value).unwrap();
            self
        }

        /// Writes under `path` fail with access denied.
        pub fn deny(mut self, path: &str) -> Self {
            self.denied.insert(fold(path));
            self
        }

        /// Writes under `path` report success and change nothing.
        pub fn ignore(mut self, path: &str) -> Self {
            self.ignored.insert(fold(path));
            self
        }

        pub fn get(&self, path: &str, name: &str) -> Option<&RawValue> {
            self.values.get(&(fold(path), fold(name)))
        }

        pub fn has_key(&self, path: &str) -> bool {
            self.keys.contains(&fold(path))
        }

        fn guard(&self, path: &str) -> Result<bool, RegistryError> {
            if self.denied.contains(&fold(path)) {
                return Err(RegistryError::AccessDenied(path.to_owned()));
            }
            Ok(self.ignored.contains(&fold(path)))
        }
    }

    impl Registry for FakeRegistry {
        fn read(&self, path: &str, name: &str) -> Result<Option<RawValue>, RegistryError> {
            Ok(self.get(path, name).cloned())
        }

        fn write(&mut self, path: &str, name: &str, value: &RawValue) -> Result<(), RegistryError> {
            if self.guard(path)? {
                return Ok(());
            }
            let mut prefix = String::new();
            for part in path.split('\\') {
                if !prefix.is_empty() {
                    prefix.push('\\');
                }
                prefix.push_str(part);
                self.keys.insert(fold(&prefix));
            }
            self.values.insert((fold(path), fold(name)), value.clone());
            Ok(())
        }

        fn delete(&mut self, path: &str, name: &str) -> Result<(), RegistryError> {
            if self.guard(path)? {
                return Ok(());
            }
            self.values.remove(&(fold(path), fold(name)));
            Ok(())
        }

        fn key_exists(&self, path: &str) -> Result<bool, RegistryError> {
            Ok(self.has_key(path))
        }

        fn remove_key_if_empty(&mut self, path: &str) -> Result<bool, RegistryError> {
            let key = fold(path);
            if !self.keys.contains(&key) {
                return Ok(true);
            }
            let child = format!("{key}\\");
            let busy = self.values.keys().any(|(owner, _)| *owner == key)
                || self.keys.iter().any(|other| other.starts_with(&child));
            if busy {
                return Ok(false);
            }
            self.keys.remove(&key);
            Ok(true)
        }
    }
}
