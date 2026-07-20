use crate::{
    AbirDataset, Atom, AtomTag, ContentId, ObjectId, PayloadDescriptor, Recording, RecordingTag,
    Stream, StreamTag,
};
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PayloadAccessError {
    NotFound(ContentId),
    AtomNotFound([u8; 16]),
    LengthMismatch { expected: u64, actual: usize },
    WrongAtomKind,
}

impl fmt::Display for PayloadAccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "payload not found: {id}"),
            Self::AtomNotFound(id) => write!(f, "atom not found: {id:02x?}"),
            Self::LengthMismatch { expected, actual } => {
                write!(
                    f,
                    "payload length mismatch: expected {expected}, got {actual}"
                )
            }
            Self::WrongAtomKind => f.write_str("atom is not the requested view kind"),
        }
    }
}

pub trait PayloadLease {
    fn bytes(&self) -> &[u8];
}

pub trait PayloadAccess {
    type Lease<'a>: PayloadLease + 'a
    where
        Self: 'a;

    fn lease<'a>(
        &'a self,
        descriptor: &PayloadDescriptor,
    ) -> Result<Self::Lease<'a>, PayloadAccessError>;
}

#[derive(Clone, Copy, Debug)]
pub struct BorrowedPayload<'a> {
    content_id: ContentId,
    bytes: &'a [u8],
}

impl<'a> BorrowedPayload<'a> {
    pub const fn new(content_id: ContentId, bytes: &'a [u8]) -> Self {
        Self { content_id, bytes }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BorrowedPayloadAccess<'a> {
    payloads: &'a [BorrowedPayload<'a>],
}

impl<'a> BorrowedPayloadAccess<'a> {
    pub const fn new(payloads: &'a [BorrowedPayload<'a>]) -> Self {
        Self { payloads }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BorrowedLease<'a>(&'a [u8]);

impl PayloadLease for BorrowedLease<'_> {
    fn bytes(&self) -> &[u8] {
        self.0
    }
}

impl PayloadAccess for BorrowedPayloadAccess<'_> {
    type Lease<'a>
        = BorrowedLease<'a>
    where
        Self: 'a;

    fn lease<'a>(
        &'a self,
        descriptor: &PayloadDescriptor,
    ) -> Result<Self::Lease<'a>, PayloadAccessError> {
        let payload = self
            .payloads
            .iter()
            .find(|payload| payload.content_id == descriptor.content_id())
            .ok_or(PayloadAccessError::NotFound(descriptor.content_id()))?;
        validate_length(descriptor, payload.bytes.len())?;
        Ok(BorrowedLease(payload.bytes))
    }
}

#[derive(Clone, Debug, Default)]
pub struct InMemoryPayloadAccess {
    payloads: BTreeMap<ContentId, Vec<u8>>,
}

impl InMemoryPayloadAccess {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, content_id: ContentId, bytes: Vec<u8>) -> Option<Vec<u8>> {
        self.payloads.insert(content_id, bytes)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct InMemoryLease<'a>(&'a [u8]);

impl PayloadLease for InMemoryLease<'_> {
    fn bytes(&self) -> &[u8] {
        self.0
    }
}

impl PayloadAccess for InMemoryPayloadAccess {
    type Lease<'a>
        = InMemoryLease<'a>
    where
        Self: 'a;

    fn lease<'a>(
        &'a self,
        descriptor: &PayloadDescriptor,
    ) -> Result<Self::Lease<'a>, PayloadAccessError> {
        let bytes = self
            .payloads
            .get(&descriptor.content_id())
            .ok_or(PayloadAccessError::NotFound(descriptor.content_id()))?;
        validate_length(descriptor, bytes.len())?;
        Ok(InMemoryLease(bytes))
    }
}

fn validate_length(
    descriptor: &PayloadDescriptor,
    actual: usize,
) -> Result<(), PayloadAccessError> {
    if u64::try_from(actual).ok() == Some(descriptor.logical_bytes()) {
        Ok(())
    } else {
        Err(PayloadAccessError::LengthMismatch {
            expected: descriptor.logical_bytes(),
            actual,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct RecordingView<'a> {
    dataset: &'a AbirDataset,
    recording: &'a Recording,
}

impl<'a> RecordingView<'a> {
    pub const fn dataset(&self) -> &'a AbirDataset {
        self.dataset
    }

    pub const fn recording(&self) -> &'a Recording {
        self.recording
    }
}

#[derive(Clone, Copy, Debug)]
pub struct StreamView<'a> {
    dataset: &'a AbirDataset,
    stream: &'a Stream,
}

impl<'a> StreamView<'a> {
    pub const fn dataset(&self) -> &'a AbirDataset {
        self.dataset
    }

    pub const fn stream(&self) -> &'a Stream {
        self.stream
    }
}

#[derive(Debug)]
pub struct BlockView<'a, L> {
    dataset: &'a AbirDataset,
    atom: &'a Atom,
    descriptor: &'a PayloadDescriptor,
    lease: L,
}

impl<'a, L: PayloadLease> BlockView<'a, L> {
    pub const fn dataset(&self) -> &'a AbirDataset {
        self.dataset
    }

    pub const fn atom(&self) -> &'a Atom {
        self.atom
    }

    pub const fn descriptor(&self) -> &'a PayloadDescriptor {
        self.descriptor
    }

    pub fn bytes(&self) -> &[u8] {
        self.lease.bytes()
    }
}

#[derive(Debug)]
pub struct TensorView<'a, L> {
    block: BlockView<'a, L>,
}

impl<'a, L: PayloadLease> TensorView<'a, L> {
    pub const fn dataset(&self) -> &'a AbirDataset {
        self.block.dataset
    }

    pub fn bytes(&self) -> &[u8] {
        self.block.bytes()
    }

    pub fn descriptor(&self) -> &'a PayloadDescriptor {
        self.block.descriptor
    }
}

#[derive(Debug)]
pub struct OpenedDataset<A> {
    dataset: AbirDataset,
    access: A,
}

impl<A> OpenedDataset<A> {
    pub const fn new(dataset: AbirDataset, access: A) -> Self {
        Self { dataset, access }
    }

    pub const fn dataset(&self) -> &AbirDataset {
        &self.dataset
    }

    pub const fn access(&self) -> &A {
        &self.access
    }

    pub fn recording_view(&self, id: ObjectId<RecordingTag>) -> Option<RecordingView<'_>> {
        let recording = self
            .dataset
            .recordings()
            .iter()
            .find(|value| value.id() == id)?;
        Some(RecordingView {
            dataset: &self.dataset,
            recording,
        })
    }

    pub fn stream_view(&self, id: ObjectId<StreamTag>) -> Option<StreamView<'_>> {
        let stream = self
            .dataset
            .streams()
            .iter()
            .find(|value| value.id() == id)?;
        Some(StreamView {
            dataset: &self.dataset,
            stream,
        })
    }
}

impl<A: PayloadAccess> OpenedDataset<A> {
    pub fn block_view(
        &self,
        id: ObjectId<AtomTag>,
    ) -> Result<BlockView<'_, A::Lease<'_>>, PayloadAccessError> {
        let atom = self
            .dataset
            .atoms()
            .iter()
            .find(|value| value.id() == id)
            .ok_or(PayloadAccessError::AtomNotFound(id.to_bytes()))?;
        let descriptor = atom.payload().ok_or(PayloadAccessError::WrongAtomKind)?;
        let lease = self.access.lease(descriptor)?;
        Ok(BlockView {
            dataset: &self.dataset,
            atom,
            descriptor,
            lease,
        })
    }

    pub fn tensor_view(
        &self,
        id: ObjectId<AtomTag>,
    ) -> Result<TensorView<'_, A::Lease<'_>>, PayloadAccessError> {
        let block = self.block_view(id)?;
        if !matches!(block.atom, Atom::Tensor(_)) {
            return Err(PayloadAccessError::WrongAtomKind);
        }
        Ok(TensorView { block })
    }
}
