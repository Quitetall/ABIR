use crate::{
    AtomTag, Calibration, ClockTag, ConceptId, ContentId, ObjectId, TimeAxis, ValidationLimits,
};
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Presence {
    Present,
    AbsentAtSource,
    UnknownAtSource,
    Withheld,
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
    Ragged {
        rows: u64,
        offsets: ContentId,
    },
    SparseCoo {
        nonzero: u64,
        indices: ContentId,
    },
    SparseCsr {
        nonzero: u64,
        indptr: ContentId,
        indices: ContentId,
    },
    BlockFloatingPoint {
        block_len: u32,
        mantissa_bits: u8,
        scales: ContentId,
    },
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
                scales,
            } => block_len > 0 && (1..=32).contains(&mantissa_bits) && scales != self.content_id,
            Layout::Ragged { rows, offsets } => rows > 0 && offsets != self.content_id,
            Layout::SparseCoo { nonzero, indices } => {
                indices != self.content_id && nonzero_fits_shape(nonzero, &self.shape)
            }
            Layout::SparseCsr {
                nonzero,
                indptr,
                indices,
            } => {
                self.shape.len() == 2
                    && indptr != indices
                    && indptr != self.content_id
                    && indices != self.content_id
                    && nonzero_fits_shape(nonzero, &self.shape)
            }
            Layout::DenseRowMajor | Layout::DenseColumnMajor => true,
        }
    }
}

fn nonzero_fits_shape(nonzero: u64, shape: &[u64]) -> bool {
    shape
        .iter()
        .try_fold(1_u64, |n, extent| n.checked_mul(*extent))
        .is_some_and(|extent| nonzero <= extent)
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TableColumn {
    semantic: ConceptId,
    element: ElementType,
    nullable: bool,
}

impl TableColumn {
    pub const fn new(semantic: ConceptId, element: ElementType, nullable: bool) -> Self {
        Self {
            semantic,
            element,
            nullable,
        }
    }

    pub fn semantic(&self) -> &ConceptId {
        &self.semantic
    }
    pub const fn element(&self) -> ElementType {
        self.element
    }
    pub const fn nullable(&self) -> bool {
        self.nullable
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticAxis {
    semantic: ConceptId,
    extent: u64,
}

impl SemanticAxis {
    pub const fn new(semantic: ConceptId, extent: u64) -> Self {
        Self { semantic, extent }
    }

    pub fn semantic(&self) -> &ConceptId {
        &self.semantic
    }
    pub const fn extent(&self) -> u64 {
        self.extent
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedSemantics {
    atom_kind: ConceptId,
    element: ElementType,
    shape: Vec<u64>,
}

impl DecodedSemantics {
    pub const fn new(atom_kind: ConceptId, element: ElementType, shape: Vec<u64>) -> Self {
        Self {
            atom_kind,
            element,
            shape,
        }
    }

    pub fn atom_kind(&self) -> &ConceptId {
        &self.atom_kind
    }
    pub const fn element(&self) -> ElementType {
        self.element
    }
    pub fn shape(&self) -> &[u64] {
        &self.shape
    }

    fn is_structurally_valid(&self, limits: ValidationLimits) -> bool {
        !self.shape.is_empty()
            && self.shape.len() <= limits.max_rank
            && self.shape.iter().all(|extent| *extent > 0)
            && self
                .shape
                .iter()
                .try_fold(1_u64, |elements, extent| elements.checked_mul(*extent))
                .is_some()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlobIntegrity {
    algorithm: ConceptId,
    digest: ContentId,
}

impl BlobIntegrity {
    pub const fn new(algorithm: ConceptId, digest: ContentId) -> Self {
        Self { algorithm, digest }
    }

    pub fn algorithm(&self) -> &ConceptId {
        &self.algorithm
    }
    pub const fn digest(&self) -> ContentId {
        self.digest
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TemporalTable {
    id: ObjectId<AtomTag>,
    presence: Presence,
    payload: Option<PayloadDescriptor>,
    clock_id: ObjectId<ClockTag>,
    record_kind: ConceptId,
    columns: Vec<TableColumn>,
}

impl TemporalTable {
    pub fn new(
        id: ObjectId<AtomTag>,
        presence: Presence,
        payload: Option<PayloadDescriptor>,
        clock_id: ObjectId<ClockTag>,
        record_kind: ConceptId,
        columns: Vec<TableColumn>,
    ) -> Self {
        Self {
            id,
            presence,
            payload,
            clock_id,
            record_kind,
            columns,
        }
    }

    pub const fn clock_id(&self) -> ObjectId<ClockTag> {
        self.clock_id
    }
    pub fn record_kind(&self) -> &ConceptId {
        &self.record_kind
    }
    pub fn columns(&self) -> &[TableColumn] {
        &self.columns
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Table {
    id: ObjectId<AtomTag>,
    presence: Presence,
    payload: Option<PayloadDescriptor>,
    columns: Vec<TableColumn>,
}

impl Table {
    pub fn new(
        id: ObjectId<AtomTag>,
        presence: Presence,
        payload: Option<PayloadDescriptor>,
        columns: Vec<TableColumn>,
    ) -> Self {
        Self {
            id,
            presence,
            payload,
            columns,
        }
    }

    pub fn columns(&self) -> &[TableColumn] {
        &self.columns
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Tensor {
    id: ObjectId<AtomTag>,
    presence: Presence,
    payload: Option<PayloadDescriptor>,
    axes: Vec<SemanticAxis>,
}

impl Tensor {
    pub fn new(
        id: ObjectId<AtomTag>,
        presence: Presence,
        payload: Option<PayloadDescriptor>,
        axes: Vec<SemanticAxis>,
    ) -> Self {
        Self {
            id,
            presence,
            payload,
            axes,
        }
    }

    pub fn axes(&self) -> &[SemanticAxis] {
        &self.axes
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedBlock {
    id: ObjectId<AtomTag>,
    presence: Presence,
    payload: Option<PayloadDescriptor>,
    decoded: DecodedSemantics,
}

impl EncodedBlock {
    pub fn new(
        id: ObjectId<AtomTag>,
        presence: Presence,
        payload: Option<PayloadDescriptor>,
        decoded: DecodedSemantics,
    ) -> Self {
        Self {
            id,
            presence,
            payload,
            decoded,
        }
    }

    pub fn decoded_semantics(&self) -> &DecodedSemantics {
        &self.decoded
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlobRef {
    id: ObjectId<AtomTag>,
    presence: Presence,
    payload: Option<PayloadDescriptor>,
    media_type: String,
    integrity: BlobIntegrity,
}

impl BlobRef {
    pub fn new(
        id: ObjectId<AtomTag>,
        presence: Presence,
        payload: Option<PayloadDescriptor>,
        media_type: String,
        integrity: BlobIntegrity,
    ) -> Self {
        Self {
            id,
            presence,
            payload,
            media_type,
            integrity,
        }
    }

    pub fn media_type(&self) -> &str {
        &self.media_type
    }
    pub fn integrity(&self) -> &BlobIntegrity {
        &self.integrity
    }
}

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
        let semantic_contract_valid = match self {
            Self::SignalBlock(_) => true,
            Self::TemporalTable(table) => columns_are_valid(&table.columns),
            Self::Table(table) => columns_are_valid(&table.columns),
            Self::Tensor(tensor) => {
                !tensor.axes.is_empty() && tensor.axes.iter().all(|axis| axis.extent > 0)
            }
            Self::EncodedBlock(block) => block.decoded.is_structurally_valid(limits),
            Self::BlobRef(blob) => media_type_is_valid(&blob.media_type),
        };
        if !semantic_contract_valid {
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
            Self::TemporalTable(table) => payload_matches_columns(payload, &table.columns),
            Self::Table(table) => payload_matches_columns(payload, &table.columns),
            Self::Tensor(tensor) => axes_match_shape(&tensor.axes, payload.shape()),
            Self::EncodedBlock(_) | Self::BlobRef(_) => true,
        }
    }
}

fn columns_are_valid(columns: &[TableColumn]) -> bool {
    !columns.is_empty()
        && columns.iter().enumerate().all(|(index, column)| {
            !columns[..index]
                .iter()
                .any(|other| other.semantic == column.semantic)
        })
}

fn payload_matches_columns(payload: &PayloadDescriptor, columns: &[TableColumn]) -> bool {
    payload.shape.len() == 2 && payload.shape[1] == columns.len() as u64
}

fn axes_match_shape(axes: &[SemanticAxis], shape: &[u64]) -> bool {
    axes.len() == shape.len()
        && axes
            .iter()
            .zip(shape)
            .all(|(axis, extent)| axis.extent > 0 && axis.extent == *extent)
}

fn media_type_is_valid(media_type: &str) -> bool {
    let Some((type_name, subtype)) = media_type.split_once('/') else {
        return false;
    };
    !type_name.is_empty()
        && !subtype.is_empty()
        && !media_type
            .chars()
            .any(|character| character.is_ascii_control() || character.is_ascii_whitespace())
}
