use std::collections::HashMap;
use std::fmt;
use std::marker::PhantomData;

/// Stable identifier derived from an asset path.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AssetId(u64);

impl AssetId {
    pub fn from_path(path: &str) -> Self {
        let mut hash = 0xcbf29ce484222325_u64;
        for byte in path.replace('\\', "/").to_lowercase().bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        Self(hash)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// Type-safe reference to a loaded asset.
#[derive(Debug, Eq, Hash, PartialEq)]
pub struct Handle<T> {
    id: AssetId,
    marker: PhantomData<fn() -> T>,
}

impl<T> Copy for Handle<T> {}

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Handle<T> {
    pub const fn id(self) -> AssetId {
        self.id
    }
}

#[derive(Debug)]
struct AssetEntry<T> {
    path: String,
    value: T,
}

/// In-memory typed asset registry.
#[derive(Debug)]
pub struct Assets<T> {
    entries: HashMap<AssetId, AssetEntry<T>>,
    paths: HashMap<String, AssetId>,
}

impl<T> Default for Assets<T> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            paths: HashMap::new(),
        }
    }
}

/// Errors returned by asset loading.
#[derive(Debug)]
pub enum AssetError<E> {
    Loader(E),
}

impl<E: fmt::Display> fmt::Display for AssetError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Loader(error) => write!(formatter, "asset loader failed: {error}"),
        }
    }
}

impl<E: fmt::Debug + fmt::Display> std::error::Error for AssetError<E> {}

impl<T> Assets<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, path: impl Into<String>, value: T) -> Handle<T> {
        let path = normalize_path(&path.into());
        let id = AssetId::from_path(&path);
        self.entries.insert(
            id,
            AssetEntry {
                path: path.clone(),
                value,
            },
        );
        self.paths.insert(path, id);
        Handle {
            id,
            marker: PhantomData,
        }
    }

    pub fn load_with<E, F>(
        &mut self,
        path: impl Into<String>,
        loader: F,
    ) -> Result<Handle<T>, AssetError<E>>
    where
        F: FnOnce(&str) -> Result<T, E>,
    {
        let path = normalize_path(&path.into());
        if let Some(id) = self.paths.get(&path).copied() {
            return Ok(Handle {
                id,
                marker: PhantomData,
            });
        }
        let value = loader(&path).map_err(AssetError::Loader)?;
        Ok(self.insert(path, value))
    }

    pub fn get(&self, handle: Handle<T>) -> Option<&T> {
        self.entries.get(&handle.id).map(|entry| &entry.value)
    }

    pub fn get_mut(&mut self, handle: Handle<T>) -> Option<&mut T> {
        self.entries
            .get_mut(&handle.id)
            .map(|entry| &mut entry.value)
    }

    pub fn path(&self, handle: Handle<T>) -> Option<&str> {
        self.entries
            .get(&handle.id)
            .map(|entry| entry.path.as_str())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::{AssetId, Assets};

    #[test]
    fn asset_handles_are_typed_and_deduplicated_by_path() {
        let mut assets = Assets::<String>::new();
        let first = assets
            .load_with("textures\\hero.txt", |path| Ok::<_, ()>(path.to_owned()))
            .expect("load");
        let second = assets
            .load_with("textures/hero.txt", |path| Ok::<_, ()>(path.to_owned()))
            .expect("cached load");

        assert_eq!(first.id(), second.id());
        assert_eq!(assets.len(), 1);
        assert_eq!(assets.get(first), Some(&"textures/hero.txt".to_owned()));
        assert_eq!(AssetId::from_path("A\\B"), AssetId::from_path("a/b"));
    }
}
