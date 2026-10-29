//! A tiny, dependency-free inline vector.
//!
//! `regie-core` is `no_std` and allocation-free by default, but several APIs
//! need a small, bounded sequence of values (for example the terms of a
//! filter intent). [`SmallVec`] provides exactly that: a fixed-capacity,
//! inline, `Copy`-friendly sequence with no allocator involved.

use core::fmt;

/// A sequence of at most `N` elements, stored inline.
///
/// ```
/// use regie_core::SmallVec;
///
/// let mut v: SmallVec<u8, 4> = SmallVec::new();
/// assert!(v.push(b'a'));
/// assert_eq!(v.as_slice(), &[b'a']);
/// ```
#[derive(Clone, Copy)]
pub struct SmallVec<T: Copy + Default, const N: usize> {
    items: [T; N],
    len: usize,
}

impl<T: Copy + Default, const N: usize> SmallVec<T, N> {
    /// Creates an empty vector.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            items: [T::default(); N],
            len: 0,
        }
    }

    /// The number of elements currently stored.
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether the vector holds no elements.
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The maximum number of elements this vector can hold.
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Appends `value`, returning `false` when the vector is full.
    pub fn push(&mut self, value: T) -> bool {
        if self.len == N {
            return false;
        }
        self.items[self.len] = value;
        self.len += 1;
        true
    }

    /// Removes every element.
    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// The stored elements as a slice.
    pub fn as_slice(&self) -> &[T] {
        &self.items[..self.len]
    }

    /// The stored elements as a mutable slice.
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.items[..self.len]
    }

    /// Returns the element at `index`, if present.
    pub fn get(&self, index: usize) -> Option<&T> {
        if index < self.len {
            Some(&self.items[index])
        } else {
            None
        }
    }

    /// Iterates over the stored elements.
    pub fn iter(&self) -> core::slice::Iter<'_, T> {
        self.as_slice().iter()
    }

    /// Returns `true` when `value` is stored in the vector.
    pub fn contains(&self, value: &T) -> bool
    where
        T: PartialEq,
    {
        self.as_slice().contains(value)
    }
}

impl<T: Copy + Default + PartialEq, const N: usize> PartialEq for SmallVec<T, N> {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl<T: Copy + Default + Eq, const N: usize> Eq for SmallVec<T, N> {}

impl<T: Copy + Default + fmt::Debug, const N: usize> fmt::Debug for SmallVec<T, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl<T: Copy + Default, const N: usize> Default for SmallVec<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy + Default, const N: usize> FromIterator<T> for SmallVec<T, N> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut out = Self::new();
        for item in iter {
            if !out.push(item) {
                break;
            }
        }
        out
    }
}

#[cfg(feature = "serde")]
impl<T: serde::Serialize + Copy + Default, const N: usize> serde::Serialize for SmallVec<T, N> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(self.len()))?;
        for item in self.iter() {
            seq.serialize_element(item)?;
        }
        seq.end()
    }
}

#[cfg(feature = "serde")]
impl<'de, T: serde::Deserialize<'de> + Copy + Default, const N: usize> serde::Deserialize<'de>
    for SmallVec<T, N>
{
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor<T, const N: usize>(core::marker::PhantomData<T>);

        impl<'de, T: serde::Deserialize<'de> + Copy + Default, const N: usize>
            serde::de::Visitor<'de> for Visitor<T, N>
        {
            type Value = SmallVec<T, N>;

            fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "a sequence of at most {N} elements")
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut out = SmallVec::new();
                while let Some(item) = seq.next_element::<T>()? {
                    if !out.push(item) {
                        return Err(serde::de::Error::custom("too many elements"));
                    }
                }
                Ok(out)
            }
        }

        deserializer.deserialize_seq(Visitor::<T, N>(core::marker::PhantomData))
    }
}
