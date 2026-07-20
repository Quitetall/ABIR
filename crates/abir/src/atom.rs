use crate::{AtomTag, Calibration, ConceptId, ContentId, ObjectId, TimeAxis, ValidationLimits};
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Presence {
    Present,
    Missing,
    Unknown,
    Redacted,
    NotApplicable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ElementType {
    I8,
    I16,
    I24,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F16,
    F32,
    F64,
    Bool,
    Utf8,
    Bytes,
}

impl ElementType {
    pub const fn byte_width(self) -> Option<u64> {
        match self {
            Self::I8 | Self::U8 | Self::Bool | Self::Bytes => Some(1),
            Self::I16 | Self::U16 | Self::F16 => Some(2),
            Self::I24 => Some(3),
            Self::I32 | Self::U32 | Self::F32 => Some(4),
            Self::I64 | Self::U64 | Self::F64 => Some(8),
            Self::Utf8 => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ByteOrder {
    Little,
    Big,
    NotApplicable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Layout {
    DenseRowMajor,
    DenseColumnMajor,
    Ragged { rows: u64 },
    SparseCoo { nonzero: u64 },
    SparseCsr { nonzero: u64 },
    BlockFloatingPoint { block_len: u32, mantissa_bits: u8 },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayloadDescriptor {
    content_id: ContentId,
    logical_bytes: u64,
    element: ElementType,
    byte_order: ByteOrder,
    shape: Vec<u64>,
    layout: Layout,
    encoding: Option<ConceptId>,
    media_type: Option<String>,
}

impl PayloadDescriptor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        content_id: ContentId,
        logical_bytes: u64,
        element: ElementType,
        byte_order: ByteOrder,
        shape: Vec<u64>,
        layout: Layout,
        encoding: Option<ConceptId>,
        media_type: Option<String>,
    ) -> Self {
        Self {
            content_id,
            logical_bytes,
            element,
            byte_order,
            shape,
            layout,
            encoding,
            media_type,
        }
    }

    pub const fn content_id(&self) -> ContentId {
        self.content_id
    }
    pub const fn logical_bytes(&self) -> u64 {
        self.logical_bytes
    }
    pub fn set_logical_bytes(&mut self, logical_bytes: u64) {
        self.logical_bytes = logical_bytes;
    }
    pub const fn element(&self) -> ElementType {
        self.element
    }
    pub const fn byte_order(&self) -> ByteOrder {
        self.byte_order
    }
    pub fn shape(&self) -> &[u64] {
        &self.shape
    }
    pub fn layout(&self) -> &Layout {
        &self.layout
    }
    pub fn encoding(&self) -> Option<&ConceptId> {
        self.encoding.as_ref()
    }
    pub fn media_type(&self) -> Option<&str> {
        self.media_type.as_deref()
    }

    pub(crate) fn is_structurally_valid(&self, limits: ValidationLimits) -> bool {
        if self.shape.len() > limits.max_rank
            || self.logical_bytes > limits.max_logical_payload_bytes
        {
            return false;
        }
        if matches!(
            self.layout,
            Layout::DenseRowMajor | Layout::DenseColumnMajor
        ) {
            let Some(width) = self.element.byte_width() else {
                return false;
            };
            let elements = self
                .shape
                .iter()
                .try_fold(1_u64, |n, extent| n.checked_mul(*extent));
            return elements.and_then(|n| n.checked_mul(width)) == Some(self.logical_bytes);
        }
        match self.layout {
            Layout::BlockFloatingPoint {
                block_len,
                mantissa_bits,
            } => block_len > 0 && (1..=32).contains(&mantissa_bits),
            Layout::Ragged { rows } => rows > 0,
            Layout::SparseCoo { nonzero } | Layout::SparseCsr { nonzero } => self
                .shape
                .iter()
                .try_fold(1_u64, |n, extent| n.checked_mul(*extent))
                .is_some_and(|extent| nonzero <= extent),
            Layout::DenseRowMajor | Layout::DenseColumnMajor => true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SignalBlock {
    id: ObjectId<AtomTag>,
    presence: Presence,
    payload: Option<PayloadDescriptor>,
    time_axis: TimeAxis,
    calibration: Option<Calibration>,
}

impl SignalBlock {
    pub fn new(
        id: ObjectId<AtomTag>,
        presence: Presence,
        payload: Option<PayloadDescriptor>,
        time_axis: TimeAxis,
        calibration: Option<Calibration>,
    ) -> Self {
        Self {
            id,
            presence,
            payload,
            time_axis,
            calibration,
        }
    }

    pub fn time_axis(&self) -> &TimeAxis {
        &self.time_axis
    }
    pub fn calibration(&self) -> Option<&Calibration> {
        self.calibration.as_ref()
    }
}

macro_rules! payload_atom {
    ($name:ident) => {
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct $name {
            id: ObjectId<AtomTag>,
            presence: Presence,
            payload: Option<PayloadDescriptor>,
        }
        impl $name {
            pub fn new(
                id: ObjectId<AtomTag>,
                presence: Presence,
                payload: Option<PayloadDescriptor>,
            ) -> Self {
                Self {
                    id,
                    presence,
                    payload,
                }
            }
        }
    };
}

payload_atom!(TemporalTable);
payload_atom!(Table);
payload_atom!(Tensor);
payload_atom!(EncodedBlock);
payload_atom!(BlobRef);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Atom {
    SignalBlock(SignalBlock),
    TemporalTable(TemporalTable),
    Table(Table),
    Tensor(Tensor),
    EncodedBlock(EncodedBlock),
    BlobRef(BlobRef),
}

macro_rules! atom_field {
    ($self:ident, $field:ident) => {
        match $self {
            Self::SignalBlock(v) => &v.$field,
            Self::TemporalTable(v) => &v.$field,
            Self::Table(v) => &v.$field,
            Self::Tensor(v) => &v.$field,
            Self::EncodedBlock(v) => &v.$field,
            Self::BlobRef(v) => &v.$field,
        }
    };
}

macro_rules! atom_field_mut {
    ($self:ident, $field:ident) => {
        match $self {
            Self::SignalBlock(v) => &mut v.$field,
            Self::TemporalTable(v) => &mut v.$field,
            Self::Table(v) => &mut v.$field,
            Self::Tensor(v) => &mut v.$field,
            Self::EncodedBlock(v) => &mut v.$field,
            Self::BlobRef(v) => &mut v.$field,
        }
    };
}

impl Atom {
    pub fn id(&self) -> ObjectId<AtomTag> {
        *atom_field!(self, id)
    }
    pub fn presence(&self) -> Presence {
        *atom_field!(self, presence)
    }
    pub fn set_presence(&mut self, presence: Presence) {
        *atom_field_mut!(self, presence) = presence;
    }
    pub fn payload(&self) -> Option<&PayloadDescriptor> {
        atom_field!(self, payload).as_ref()
    }
    pub fn payload_mut(&mut self) -> Option<&mut PayloadDescriptor> {
        atom_field_mut!(self, payload).as_mut()
    }

    pub(crate) fn is_structurally_valid(&self, limits: ValidationLimits) -> bool {
        let presence_matches = match (self.presence(), self.payload()) {
            (Presence::Present, Some(_)) => true,
            (Presence::Present, None) => false,
            (_, None) => true,
            (_, Some(_)) => false,
        };
        if !presence_matches {
            return false;
        }
        let Some(payload) = self.payload() else {
            return true;
        };
        if !payload.is_structurally_valid(limits) {
            return false;
        }
        match self {
            Self::SignalBlock(block) => block
                .time_axis
                .sample_count()
                .is_ok_and(|samples| payload.shape.last().copied() == Some(samples)),
            Self::BlobRef(_) => payload.media_type().is_some(),
            _ => true,
        }
    }
}
