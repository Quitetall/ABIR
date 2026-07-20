use core::fmt;
use core::marker::PhantomData;

macro_rules! semantic_tags {
    ($($name:ident),+ $(,)?) => {
        $(
            #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
            pub enum $name {}
        )+
    };
}

semantic_tags!(
    DatasetTag,
    RecordingTag,
    StreamTag,
    AtomTag,
    ClockTag,
    CoordinateFrameTag,
    ChannelBasisTag,
    PolicyTag,
    ProofTag,
    DerivationTag,
);

/// Typed 128-bit semantic identity.
pub struct ObjectId<T> {
    bytes: [u8; 16],
    marker: PhantomData<fn() -> T>,
}

impl<T> ObjectId<T> {
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self {
            bytes,
            marker: PhantomData,
        }
    }

    pub const fn to_bytes(self) -> [u8; 16] {
        self.bytes
    }

    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.bytes
    }
}

impl<T> Clone for ObjectId<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for ObjectId<T> {}

impl<T> PartialEq for ObjectId<T> {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes
    }
}

impl<T> Eq for ObjectId<T> {}

impl<T> PartialOrd for ObjectId<T> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for ObjectId<T> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.bytes.cmp(&other.bytes)
    }
}

impl<T> core::hash::Hash for ObjectId<T> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.bytes.hash(state);
    }
}

impl<T> fmt::Debug for ObjectId<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl<T> fmt::Display for ObjectId<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_hex(f, &self.bytes)
    }
}

/// Generation-local lookup key. Handles have no serialized meaning.
pub struct Handle<T> {
    value: u32,
    marker: PhantomData<fn() -> T>,
}

impl<T> Handle<T> {
    pub const fn new(value: u32) -> Self {
        Self {
            value,
            marker: PhantomData,
        }
    }

    pub const fn get(self) -> u32 {
        self.value
    }
}

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Handle<T> {}

impl<T> fmt::Debug for Handle<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Handle").field(&self.value).finish()
    }
}

impl<T> PartialEq for Handle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T> Eq for Handle<T> {}

macro_rules! digest_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name([u8; 32]);

        impl $name {
            pub const fn from_bytes(bytes: [u8; 32]) -> Self {
                Self(bytes)
            }

            pub const fn to_bytes(self) -> [u8; 32] {
                self.0
            }

            pub const fn as_bytes(&self) -> &[u8; 32] {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write_hex(f, &self.0)
            }
        }
    };
}

digest_id!(
    ContentId,
    "BLAKE3-256 identity of canonical logical content."
);
digest_id!(
    StorageId,
    "Physical identity reserved for the storage layer."
);

fn write_hex(f: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    for byte in bytes {
        write!(f, "{byte:02x}")?;
    }
    Ok(())
}
