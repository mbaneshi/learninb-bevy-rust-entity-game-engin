#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![expect(
    unsafe_code,
    reason = "Some utilities, such as futures and cells, require unsafe code."
)]
#![doc(
    html_logo_url = "https://bevyengine.org/assets/icon.png",
    html_favicon_url = "https://bevyengine.org/assets/icon.png"
)]
#![cfg_attr(not(feature = "std"), no_std)]

//! General utilities for first-party [Bevy] engine crates.
//!
//! [Bevy]: https://bevyengine.org/

#[cfg(feature = "alloc")]
extern crate alloc;

/// The utilities prelude.
///
/// This includes the most common types in this crate, re-exported for your convenience.
pub mod prelude {
    pub use crate::default;
}

pub mod futures;
pub mod synccell;
pub mod syncunsafecell;

mod default;
mod object_safe;
pub use object_safe::assert_object_safe;
mod once;
#[cfg(feature = "std")]
mod parallel_queue;
mod time;

pub use ahash::{AHasher, RandomState};
pub use bevy_utils_proc_macros::*;
pub use default::default;
pub use hashbrown;
#[cfg(feature = "std")]
pub use parallel_queue::*;
pub use time::*;
pub use tracing;

#[cfg(feature = "alloc")]
use alloc::boxed::Box;

use core::{
    any::TypeId,
    fmt::Debug,
    hash::{BuildHasher, BuildHasherDefault, Hash, Hasher},
    marker::PhantomData,
    mem::ManuallyDrop,
    ops::Deref,
};
use hashbrown::hash_map::RawEntryMut;

#[cfg(not(target_arch = "wasm32"))]
mod conditional_send {
    /// Use [`ConditionalSend`] to mark an optional Send trait bound. Useful as on certain platforms (eg. Wasm),
    /// futures aren't Send.
    pub trait ConditionalSend: Send {}
    impl<T: Send> ConditionalSend for T {}
}

#[cfg(target_arch = "wasm32")]
#[expect(missing_docs, reason = "Not all docs are written yet (#3492).")]
mod conditional_send {
    pub trait ConditionalSend {}
    impl<T> ConditionalSend for T {}
}

pub use conditional_send::*;

/// Use [`ConditionalSendFuture`] for a future with an optional Send trait bound, as on certain platforms (eg. Wasm),
/// futures aren't Send.
pub trait ConditionalSendFuture: core::future::Future + ConditionalSend {}
impl<T: core::future::Future + ConditionalSend> ConditionalSendFuture for T {}

/// An owned and dynamically typed Future used when you can't statically type your result or need to add some indirection.
#[cfg(feature = "alloc")]
pub type BoxedFuture<'a, T> = core::pin::Pin<Box<dyn ConditionalSendFuture<Output = T> + 'a>>;

/// A shortcut alias for [`hashbrown::hash_map::Entry`].
pub type Entry<'a, K, V, S = BuildHasherDefault<AHasher>> = hashbrown::hash_map::Entry<'a, K, V, S>;

/// A hasher builder that will create a fixed hasher.
#[derive(Debug, Clone, Default)]
pub struct FixedState;

impl BuildHasher for FixedState {
    type Hasher = AHasher;

    #[inline]
    fn build_hasher(&self) -> AHasher {
        RandomState::with_seeds(
            0b10010101111011100000010011000100,
            0b00000011001001101011001001111000,
            0b11001111011010110111100010110101,
            0b00000100001111100011010011010101,
        )
        .build_hasher()
    }
}

/// A [`HashMap`][hashbrown::HashMap] implementing aHash, a high
/// speed keyed hashing algorithm intended for use in in-memory hashmaps.
///
/// aHash is designed for performance and is NOT cryptographically secure.
///
/// Within the same execution of the program iteration order of different
/// `HashMap`s only depends on the order of insertions and deletions,
/// but it will not be stable between multiple executions of the program.
pub type HashMap<K, V> = hashbrown::HashMap<K, V, BuildHasherDefault<AHasher>>;

/// A stable hash map implementing aHash, a high speed keyed hashing algorithm
/// intended for use in in-memory hashmaps.
///
/// Unlike [`HashMap`] the iteration order stability extends between executions
/// using the same Bevy version on the same device.
///
/// aHash is designed for performance and is NOT cryptographically secure.
#[deprecated(
    note = "Will be required to use the hash library of your choice. Alias for: hashbrown::HashMap<K, V, FixedState>"
)]
pub type StableHashMap<K, V> = hashbrown::HashMap<K, V, FixedState>;

/// A [`HashSet`][hashbrown::HashSet] implementing aHash, a high
/// speed keyed hashing algorithm intended for use in in-memory hashmaps.
///
/// aHash is designed for performance and is NOT cryptographically secure.
///
/// Within the same execution of the program iteration order of different
/// `HashSet`s only depends on the order of insertions and deletions,
/// but it will not be stable between multiple executions of the program.
pub type HashSet<K> = hashbrown::HashSet<K, BuildHasherDefault<AHasher>>;

/// A stable hash set implementing aHash, a high speed keyed hashing algorithm
/// intended for use in in-memory hashmaps.
///
/// Unlike [`HashMap`] the iteration order stability extends between executions
/// using the same Bevy version on the same device.
///
/// aHash is designed for performance and is NOT cryptographically secure.
#[deprecated(
    note = "Will be required to use the hash library of your choice. Alias for: hashbrown::HashSet<K, FixedState>"
)]
pub type StableHashSet<K> = hashbrown::HashSet<K, FixedState>;

/// A pre-hashed value of a specific type. Pre-hashing enables memoization of hashes that are expensive to compute.
///
/// It also enables faster [`PartialEq`] comparisons by short circuiting on hash equality.
/// See [`PassHash`] and [`PassHasher`] for a "pass through" [`BuildHasher`] and [`Hasher`] implementation
/// designed to work with [`Hashed`]
/// See [`PreHashMap`] for a hashmap pre-configured to use [`Hashed`] keys.
pub struct Hashed<V, H = FixedState> {
    hash: u64,
    value: V,
    marker: PhantomData<H>,
}

impl<V: Hash, H: BuildHasher + Default> Hashed<V, H> {
    /// Pre-hashes the given value using the [`BuildHasher`] configured in the [`Hashed`] type.
    pub fn new(value: V) -> Self {
        Self {
            hash: H::default().hash_one(&value),
            value,
            marker: PhantomData,
        }
    }

    /// The pre-computed hash.
    #[inline]
    pub fn hash(&self) -> u64 {
        self.hash
    }
}

impl<V, H> Hash for Hashed<V, H> {
    #[inline]
    fn hash<R: Hasher>(&self, state: &mut R) {
        state.write_u64(self.hash);
    }
}

impl<V, H> Deref for Hashed<V, H> {
    type Target = V;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<V: PartialEq, H> PartialEq for Hashed<V, H> {
    /// A fast impl of [`PartialEq`] that first checks that `other`'s pre-computed hash
    /// matches this value's pre-computed hash.
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash && self.value.eq(&other.value)
    }
}

impl<V: Debug, H> Debug for Hashed<V, H> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Hashed")
            .field("hash", &self.hash)
            .field("value", &self.value)
            .finish()
    }
}

impl<V: Clone, H> Clone for Hashed<V, H> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            hash: self.hash,
            value: self.value.clone(),
            marker: PhantomData,
        }
    }
}

impl<V: Copy, H> Copy for Hashed<V, H> {}

impl<V: Eq, H> Eq for Hashed<V, H> {}

/// A [`BuildHasher`] that results in a [`PassHasher`].
#[derive(Default, Clone)]
pub struct PassHash;

impl BuildHasher for PassHash {
    type Hasher = PassHasher;

    fn build_hasher(&self) -> Self::Hasher {
        PassHasher::default()
    }
}

/// A no-op hash that only works on `u64`s. Will panic if attempting to
/// hash a type containing non-u64 fields.
#[derive(Debug, Default)]
pub struct PassHasher {
    hash: u64,
}

impl Hasher for PassHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }

    fn write(&mut self, _bytes: &[u8]) {
        panic!("can only hash u64 using PassHasher");
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.hash = i;
    }
}

/// A [`HashMap`] pre-configured to use [`Hashed`] keys and [`PassHash`] passthrough hashing.
/// Iteration order only depends on the order of insertions and deletions.
pub type PreHashMap<K, V> = hashbrown::HashMap<Hashed<K>, V, PassHash>;

/// Extension methods intended to add functionality to [`PreHashMap`].
pub trait PreHashMapExt<K, V> {
    /// Tries to get or insert the value for the given `key` using the pre-computed hash first.
    /// If the [`PreHashMap`] does not already contain the `key`, it will clone it and insert
    /// the value returned by `func`.
    fn get_or_insert_with<F: FnOnce() -> V>(&mut self, key: &Hashed<K>, func: F) -> &mut V;
}

impl<K: Hash + Eq + PartialEq + Clone, V> PreHashMapExt<K, V> for PreHashMap<K, V> {
    #[inline]
    fn get_or_insert_with<F: FnOnce() -> V>(&mut self, key: &Hashed<K>, func: F) -> &mut V {
        let entry = self
            .raw_entry_mut()
            .from_key_hashed_nocheck(key.hash(), key);
        match entry {
            RawEntryMut::Occupied(entry) => entry.into_mut(),
            RawEntryMut::Vacant(entry) => {
                let (_, value) = entry.insert_hashed_nocheck(key.hash(), key.clone(), func());
                value
            }
        }
    }
}

/// A specialized hashmap type with Key of [`TypeId`]
/// Iteration order only depends on the order of insertions and deletions.
pub type TypeIdMap<V> = hashbrown::HashMap<TypeId, V, NoOpHash>;

/// [`BuildHasher`] for types that already contain a high-quality hash.
#[derive(Clone, Default)]
pub struct NoOpHash;

impl BuildHasher for NoOpHash {
    type Hasher = NoOpHasher;

    fn build_hasher(&self) -> Self::Hasher {
        NoOpHasher(0)
    }
}

#[doc(hidden)]
pub struct NoOpHasher(u64);

// This is for types that already contain a high-quality hash and want to skip
// re-hashing that hash.
impl Hasher for NoOpHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        // This should never be called by consumers. Prefer to call `write_u64` instead.
        // Don't break applications (slower fallback, just check in test):
        self.0 = bytes.iter().fold(self.0, |hash, b| {
            hash.rotate_left(8).wrapping_add(*b as u64)
        });
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.0 = i;
    }
}

/// A type which calls a function when dropped.
/// This can be used to ensure that cleanup code is run even in case of a panic.
///
/// Note that this only works for panics that [unwind](https://doc.rust-lang.org/nomicon/unwinding.html)
/// -- any code within `OnDrop` will be skipped if a panic does not unwind.
/// In most cases, this will just work.
///
/// # Examples
///
/// ```
/// # use bevy_utils::OnDrop;
/// # fn test_panic(do_panic: bool, log: impl FnOnce(&str)) {
/// // This will print a message when the variable `_catch` gets dropped,
/// // even if a panic occurs before we reach the end of this scope.
/// // This is similar to a `try ... catch` block in languages such as C++.
/// let _catch = OnDrop::new(|| log("Oops, a panic occurred and this function didn't complete!"));
///
/// // Some code that may panic...
/// // ...
/// # if do_panic { panic!() }
///
/// // Make sure the message only gets printed if a panic occurs.
/// // If we remove this line, then the message will be printed regardless of whether a panic occurs
/// // -- similar to a `try ... finally` block.
/// core::mem::forget(_catch);
/// # }
/// #
/// # test_panic(false, |_| unreachable!());
/// # let mut did_log = false;
/// # std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
/// #   test_panic(true, |_| did_log = true);
/// # }));
/// # assert!(did_log);
/// ```
pub struct OnDrop<F: FnOnce()> {
    callback: ManuallyDrop<F>,
}

impl<F: FnOnce()> OnDrop<F> {
    /// Returns an object that will invoke the specified callback when dropped.
    pub fn new(callback: F) -> Self {
        Self {
            callback: ManuallyDrop::new(callback),
        }
    }
}

impl<F: FnOnce()> Drop for OnDrop<F> {
    fn drop(&mut self) {
        // SAFETY: We may move out of `self`, since this instance can never be observed after it's dropped.
        let callback = unsafe { ManuallyDrop::take(&mut self.callback) };
        callback();
    }
}

/// Calls the [`tracing::info!`] macro on a value.
pub fn info<T: Debug>(data: T) {
    tracing::info!("{:?}", data);
}

/// Calls the [`tracing::debug!`] macro on a value.
pub fn dbg<T: Debug>(data: T) {
    tracing::debug!("{:?}", data);
}

/// Processes a [`Result`] by calling the [`tracing::warn!`] macro in case of an [`Err`] value.
pub fn warn<E: Debug>(result: Result<(), E>) {
    if let Err(warn) = result {
        tracing::warn!("{:?}", warn);
    }
}

/// Processes a [`Result`] by calling the [`tracing::error!`] macro in case of an [`Err`] value.
pub fn error<E: Debug>(result: Result<(), E>) {
    if let Err(error) = result {
        tracing::error!("{:?}", error);
    }
}

/// Like [`tracing::trace`], but conditional on cargo feature `detailed_trace`.
#[macro_export]
macro_rules! detailed_trace {
    ($($tts:tt)*) => {
        if cfg!(feature = "detailed_trace") {
            $crate::tracing::trace!($($tts)*);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use static_assertions::assert_impl_all;

    // Check that the HashMaps are Clone if the key/values are Clone
    assert_impl_all!(PreHashMap::<u64, usize>: Clone);

    #[test]
    fn fast_typeid_hash() {
        struct Hasher;

        impl core::hash::Hasher for Hasher {
            fn finish(&self) -> u64 {
                0
            }
            fn write(&mut self, _: &[u8]) {
                panic!("Hashing of core::any::TypeId changed");
            }
            fn write_u64(&mut self, _: u64) {}
        }

        Hash::hash(&TypeId::of::<()>(), &mut Hasher);
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn stable_hash_within_same_program_execution() {
        use alloc::vec::Vec;

        let mut map_1 = HashMap::new();
        let mut map_2 = HashMap::new();
        for i in 1..10 {
            map_1.insert(i, i);
            map_2.insert(i, i);
        }
        assert_eq!(
            map_1.iter().collect::<Vec<_>>(),
            map_2.iter().collect::<Vec<_>>()
        );
    }
}