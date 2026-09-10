use std::collections::HashMap;
use std::fmt;
use std::marker::PhantomData;

/// Normalized asset key representing canonical path identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AssetKey(String);

impl AssetKey {
    pub fn new(path: &str) -> Self {
        Self(normalize_path(path))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Stable 64-bit identifier derived from normalized asset path.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AssetId(u64);

impl AssetId {
    pub fn from_path(path: &str) -> Self {
        let normalized = normalize_path(path);
        let mut hash = 0xcbf29ce484222325_u64;
        for byte in normalized.bytes() {
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

    pub fn from_id(id: AssetId) -> Self {
        Self {
            id,
            marker: PhantomData,
        }
    }
}

/// Current status of an asset in the asset server / manager.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssetState {
    Unloaded,
    Loading,
    Loaded,
    Failed(String),
}

#[derive(Debug)]
struct AssetEntry<T> {
    path: String,
    value: T,
    state: AssetState,
}

/// In-memory typed asset registry with path canonicalization and collision protection.
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

/// Errors returned by asset loading or registration.
#[derive(Debug, PartialEq, Eq)]
pub enum AssetError<E = String> {
    Loader(E),
    Collision { path: String, existing_path: String },
    NotFound(AssetId),
}

impl<E: fmt::Display> fmt::Display for AssetError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Loader(error) => write!(formatter, "asset loader failed: {error}"),
            Self::Collision {
                path,
                existing_path,
            } => write!(
                formatter,
                "asset path collision: '{path}' collides with existing '{existing_path}'"
            ),
            Self::NotFound(id) => write!(formatter, "asset not found for ID: {id:?}"),
        }
    }
}

impl<E: fmt::Debug + fmt::Display> std::error::Error for AssetError<E> {}

impl<T> Assets<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(
        &mut self,
        path: impl Into<String>,
        value: T,
    ) -> Result<Handle<T>, AssetError<String>> {
        let canonical_path = normalize_path(&path.into());
        let id = AssetId::from_path(&canonical_path);

        if let Some(existing) = self.entries.get(&id) {
            if existing.path != canonical_path {
                return Err(AssetError::Collision {
                    path: canonical_path,
                    existing_path: existing.path.clone(),
                });
            }
        }

        self.entries.insert(
            id,
            AssetEntry {
                path: canonical_path.clone(),
                value,
                state: AssetState::Loaded,
            },
        );
        self.paths.insert(canonical_path, id);

        Ok(Handle::from_id(id))
    }

    pub fn load_with<E, F>(
        &mut self,
        path: impl Into<String>,
        loader: F,
    ) -> Result<Handle<T>, AssetError<E>>
    where
        F: FnOnce(&str) -> Result<T, E>,
    {
        let canonical_path = normalize_path(&path.into());
        if let Some(id) = self.paths.get(&canonical_path).copied() {
            return Ok(Handle::from_id(id));
        }

        let value = loader(&canonical_path).map_err(AssetError::Loader)?;
        let id = AssetId::from_path(&canonical_path);

        if let Some(existing) = self.entries.get(&id) {
            if existing.path != canonical_path {
                return Err(AssetError::Collision {
                    path: canonical_path,
                    existing_path: existing.path.clone(),
                });
            }
        }

        self.entries.insert(
            id,
            AssetEntry {
                path: canonical_path.clone(),
                value,
                state: AssetState::Loaded,
            },
        );
        self.paths.insert(canonical_path, id);

        Ok(Handle::from_id(id))
    }

    pub fn get(&self, handle: Handle<T>) -> Option<&T> {
        self.entries.get(&handle.id).map(|entry| &entry.value)
    }

    pub fn get_mut(&mut self, handle: Handle<T>) -> Option<&mut T> {
        self.entries
            .get_mut(&handle.id)
            .map(|entry| &mut entry.value)
    }

    pub fn state(&self, handle: Handle<T>) -> AssetState {
        self.entries
            .get(&handle.id)
            .map_or(AssetState::Unloaded, |entry| entry.state.clone())
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

/// Sanitizes and canonicalizes asset relative paths to prevent directory traversal and casing mismatch.
pub fn normalize_path(path: &str) -> String {
    let raw = path.replace('\\', "/");
    let without_drive = if let Some(idx) = raw.find(':') {
        &raw[idx + 1..]
    } else {
        &raw
    };

    let mut parts = Vec::new();
    for part in without_drive.split('/') {
        match part {
            "" | "." => continue,
            ".." => {
                parts.pop();
            }
            other => parts.push(other.to_lowercase()),
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::{AssetId, Assets, normalize_path};

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

    #[test]
    fn path_normalization_prevents_traversal_and_casing_mismatch() {
        assert_eq!(
            normalize_path("textures\\../textures/HERO.PNG"),
            "textures/hero.png"
        );
        assert_eq!(
            normalize_path("C:\\Game/assets/../textures/hero.png"),
            "game/textures/hero.png"
        );
        assert_eq!(normalize_path("../secret.txt"), "secret.txt");
    }
}
