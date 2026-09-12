pub mod geometry_codec;

use std::collections::HashMap;
use std::fmt;
use std::marker::PhantomData;

const MAX_ASSET_PATH_BYTES: usize = 4096;

/// Errors produced while converting an external path into an engine asset key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssetPathError {
    Empty,
    TooLong,
    Absolute,
    WindowsPrefix,
    ParentTraversal,
    NulByte,
}

impl fmt::Display for AssetPathError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(formatter, "asset path is empty"),
            Self::TooLong => write!(formatter, "asset path exceeds {MAX_ASSET_PATH_BYTES} bytes"),
            Self::Absolute => write!(formatter, "absolute asset paths are not allowed"),
            Self::WindowsPrefix => write!(formatter, "Windows drive/UNC prefixes are not allowed"),
            Self::ParentTraversal => {
                write!(formatter, "asset path attempts to escape its virtual root")
            }
            Self::NulByte => write!(formatter, "asset path contains a NUL byte"),
        }
    }
}

impl std::error::Error for AssetPathError {}

/// Normalized, validated asset key representing canonical virtual-path identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AssetKey(String);

impl AssetKey {
    pub fn new(path: &str) -> Result<Self, AssetPathError> {
        normalize_path(path).map(Self)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Stable 64-bit identifier derived from a validated normalized asset path.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AssetId(u64);

impl AssetId {
    pub fn from_key(key: &AssetKey) -> Self {
        Self::from_normalized(key.as_str())
    }

    pub fn from_path(path: &str) -> Result<Self, AssetPathError> {
        let key = AssetKey::new(path)?;
        Ok(Self::from_key(&key))
    }

    fn from_normalized(path: &str) -> Self {
        let mut hash = 0xcbf29ce484222325_u64;
        for byte in path.bytes() {
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
    key: AssetKey,
    value: T,
    state: AssetState,
}

/// In-memory typed asset registry with fail-closed path validation and collision protection.
#[derive(Debug)]
pub struct Assets<T> {
    entries: HashMap<AssetId, AssetEntry<T>>,
    paths: HashMap<AssetKey, AssetId>,
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
    InvalidPath(AssetPathError),
    Collision { path: String, existing_path: String },
    NotFound(AssetId),
}

impl<E: fmt::Display> fmt::Display for AssetError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Loader(error) => write!(formatter, "asset loader failed: {error}"),
            Self::InvalidPath(error) => write!(formatter, "invalid asset path: {error}"),
            Self::Collision {
                path,
                existing_path,
            } => write!(
                formatter,
                "asset ID collision: '{path}' collides with existing '{existing_path}'"
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
        let raw = path.into();
        let key = AssetKey::new(&raw).map_err(AssetError::InvalidPath)?;
        let id = AssetId::from_key(&key);
        self.check_collision(id, &key)?;

        self.entries.insert(
            id,
            AssetEntry {
                key: key.clone(),
                value,
                state: AssetState::Loaded,
            },
        );
        self.paths.insert(key, id);
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
        let raw = path.into();
        let key = AssetKey::new(&raw).map_err(AssetError::InvalidPath)?;
        if let Some(id) = self.paths.get(&key).copied() {
            return Ok(Handle::from_id(id));
        }

        let id = AssetId::from_key(&key);
        if let Some(existing) = self.entries.get(&id) {
            if existing.key != key {
                return Err(AssetError::Collision {
                    path: key.as_str().to_owned(),
                    existing_path: existing.key.as_str().to_owned(),
                });
            }
        }

        let value = loader(key.as_str()).map_err(AssetError::Loader)?;
        self.entries.insert(
            id,
            AssetEntry {
                key: key.clone(),
                value,
                state: AssetState::Loaded,
            },
        );
        self.paths.insert(key, id);
        Ok(Handle::from_id(id))
    }

    fn check_collision<E>(&self, id: AssetId, key: &AssetKey) -> Result<(), AssetError<E>> {
        if let Some(existing) = self.entries.get(&id) {
            if existing.key != *key {
                return Err(AssetError::Collision {
                    path: key.as_str().to_owned(),
                    existing_path: existing.key.as_str().to_owned(),
                });
            }
        }
        Ok(())
    }

    pub fn get(&self, handle: Handle<T>) -> Option<&T> {
        self.entries.get(&handle.id).map(|entry| &entry.value)
    }

    pub fn get_mut(&mut self, handle: Handle<T>) -> Option<&mut T> {
        self.entries
            .get_mut(&handle.id)
            .map(|entry| &mut entry.value)
    }

    pub fn remove(&mut self, handle: Handle<T>) -> Option<T> {
        let entry = self.entries.remove(&handle.id)?;
        self.paths.remove(&entry.key);
        Some(entry.value)
    }

    pub fn state(&self, handle: Handle<T>) -> AssetState {
        self.entries
            .get(&handle.id)
            .map_or(AssetState::Unloaded, |entry| entry.state.clone())
    }

    pub fn path(&self, handle: Handle<T>) -> Option<&str> {
        self.entries.get(&handle.id).map(|entry| entry.key.as_str())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Canonicalizes a relative virtual asset path. Attempts to escape the root are rejected.
pub fn normalize_path(path: &str) -> Result<String, AssetPathError> {
    if path.is_empty() {
        return Err(AssetPathError::Empty);
    }
    if path.len() > MAX_ASSET_PATH_BYTES {
        return Err(AssetPathError::TooLong);
    }
    if path.as_bytes().contains(&0) {
        return Err(AssetPathError::NulByte);
    }

    let raw = path.replace('\\', "/");
    if raw.starts_with("//") {
        return Err(AssetPathError::WindowsPrefix);
    }
    if raw.starts_with('/') {
        return Err(AssetPathError::Absolute);
    }
    if raw.len() >= 2 && raw.as_bytes()[1] == b':' && raw.as_bytes()[0].is_ascii_alphabetic() {
        return Err(AssetPathError::WindowsPrefix);
    }

    let mut parts: Vec<String> = Vec::new();
    for part in raw.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(AssetPathError::ParentTraversal);
                }
            }
            other => parts.push(other.to_lowercase()),
        }
    }

    if parts.is_empty() {
        return Err(AssetPathError::Empty);
    }
    Ok(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::{normalize_path, AssetId, AssetPathError, Assets};

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
    fn remove_drops_value_and_frees_the_path() {
        let mut assets = Assets::<String>::new();
        let handle = assets
            .insert("meshes/hero.bin", "mesh".to_owned())
            .expect("insert");
        assert_eq!(assets.remove(handle), Some("mesh".to_owned()));
        assert!(assets.is_empty());
        let again = assets
            .insert("meshes/hero.bin", "mesh-2".to_owned())
            .expect("reinsert");
        assert_eq!(assets.get(again), Some(&"mesh-2".to_owned()));
    }

    #[test]
    fn normalization_resolves_only_in_root_parent_components() {
        assert_eq!(
            normalize_path("textures\\../textures/HERO.PNG"),
            Ok("textures/hero.png".to_owned())
        );
        assert_eq!(normalize_path("a/./b//c"), Ok("a/b/c".to_owned()));
    }

    #[test]
    fn unsafe_paths_fail_closed() {
        assert_eq!(
            normalize_path("../secret.txt"),
            Err(AssetPathError::ParentTraversal)
        );
        assert_eq!(
            normalize_path("../../secret.txt"),
            Err(AssetPathError::ParentTraversal)
        );
        assert_eq!(normalize_path("/etc/passwd"), Err(AssetPathError::Absolute));
        assert_eq!(
            normalize_path("C:\\Game\\secret.txt"),
            Err(AssetPathError::WindowsPrefix)
        );
        assert_eq!(
            normalize_path("\\\\server\\share\\x"),
            Err(AssetPathError::WindowsPrefix)
        );
        assert_eq!(normalize_path("bad\0name"), Err(AssetPathError::NulByte));
    }

    #[test]
    fn invalid_path_never_reaches_loader() {
        let mut assets = Assets::<String>::new();
        let mut called = false;
        let result = assets.load_with("../secret.txt", |_| {
            called = true;
            Ok::<_, ()>("secret".to_owned())
        });
        assert!(!called);
        assert!(result.is_err());
        assert!(assets.is_empty());
    }
}
