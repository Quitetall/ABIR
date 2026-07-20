use crate::{
    AtomTag, ChannelBasisTag, ClockTag, ConceptId, CoordinateFrameTag, ExactNumber, ObjectId,
    PolicyTag, Rational, RecordingTag, SourceKey, StreamTag,
};
use alloc::vec::Vec;
use core::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Recording {
    id: ObjectId<RecordingTag>,
    streams: Vec<ObjectId<StreamTag>>,
    source_keys: Vec<SourceKey>,
}

impl Recording {
    pub fn new(id: ObjectId<RecordingTag>, streams: Vec<ObjectId<StreamTag>>) -> Self {
        Self {
            id,
            streams,
            source_keys: Vec::new(),
        }
    }

    pub fn add_source_key(&mut self, key: SourceKey) {
        self.source_keys.push(key);
    }

    pub const fn id(&self) -> ObjectId<RecordingTag> {
        self.id
    }
    pub fn streams(&self) -> &[ObjectId<StreamTag>] {
        &self.streams
    }
    pub fn source_keys(&self) -> &[SourceKey] {
        &self.source_keys
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stream {
    id: ObjectId<StreamTag>,
    recording_id: ObjectId<RecordingTag>,
    modality: ConceptId,
    atoms: Vec<ObjectId<AtomTag>>,
    clock_id: Option<ObjectId<ClockTag>>,
    channel_basis_id: Option<ObjectId<ChannelBasisTag>>,
    policy_id: Option<ObjectId<PolicyTag>>,
}

impl Stream {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: ObjectId<StreamTag>,
        recording_id: ObjectId<RecordingTag>,
        modality: ConceptId,
        atoms: Vec<ObjectId<AtomTag>>,
        clock_id: Option<ObjectId<ClockTag>>,
        channel_basis_id: Option<ObjectId<ChannelBasisTag>>,
        policy_id: Option<ObjectId<PolicyTag>>,
    ) -> Self {
        Self {
            id,
            recording_id,
            modality,
            atoms,
            clock_id,
            channel_basis_id,
            policy_id,
        }
    }

    pub const fn id(&self) -> ObjectId<StreamTag> {
        self.id
    }
    pub const fn recording_id(&self) -> ObjectId<RecordingTag> {
        self.recording_id
    }
    pub fn modality(&self) -> &ConceptId {
        &self.modality
    }
    pub fn atoms(&self) -> &[ObjectId<AtomTag>] {
        &self.atoms
    }
    pub const fn clock_id(&self) -> Option<ObjectId<ClockTag>> {
        self.clock_id
    }
    pub fn set_clock_id(&mut self, clock_id: Option<ObjectId<ClockTag>>) {
        self.clock_id = clock_id;
    }
    pub const fn channel_basis_id(&self) -> Option<ObjectId<ChannelBasisTag>> {
        self.channel_basis_id
    }
    pub const fn policy_id(&self) -> Option<ObjectId<PolicyTag>> {
        self.policy_id
    }
    pub fn set_policy_id(&mut self, policy_id: Option<ObjectId<PolicyTag>>) {
        self.policy_id = policy_id;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Clock {
    id: ObjectId<ClockTag>,
    kind: ConceptId,
    parent_id: Option<ObjectId<ClockTag>>,
    offset: Rational,
    rate: Rational,
    uncertainty: Rational,
}

impl Clock {
    pub fn new(
        id: ObjectId<ClockTag>,
        kind: ConceptId,
        parent_id: Option<ObjectId<ClockTag>>,
        offset: Rational,
        rate: Rational,
        uncertainty: Rational,
    ) -> Self {
        Self {
            id,
            kind,
            parent_id,
            offset,
            rate,
            uncertainty,
        }
    }

    pub const fn id(&self) -> ObjectId<ClockTag> {
        self.id
    }
    pub fn kind(&self) -> &ConceptId {
        &self.kind
    }
    pub const fn parent_id(&self) -> Option<ObjectId<ClockTag>> {
        self.parent_id
    }
    pub fn set_parent_id(&mut self, parent_id: Option<ObjectId<ClockTag>>) {
        self.parent_id = parent_id;
    }
    pub const fn rate(&self) -> Rational {
        self.rate
    }
    pub const fn offset(&self) -> Rational {
        self.offset
    }
    pub const fn uncertainty(&self) -> Rational {
        self.uncertainty
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoordinateFrame {
    id: ObjectId<CoordinateFrameTag>,
    kind: ConceptId,
    parent_id: Option<ObjectId<CoordinateFrameTag>>,
    transform: Option<[ExactNumber; 16]>,
    uncertainty: Rational,
}

impl CoordinateFrame {
    pub fn new(
        id: ObjectId<CoordinateFrameTag>,
        kind: ConceptId,
        parent_id: Option<ObjectId<CoordinateFrameTag>>,
        transform: Option<[ExactNumber; 16]>,
        uncertainty: Rational,
    ) -> Self {
        Self {
            id,
            kind,
            parent_id,
            transform,
            uncertainty,
        }
    }

    pub const fn id(&self) -> ObjectId<CoordinateFrameTag> {
        self.id
    }
    pub const fn parent_id(&self) -> Option<ObjectId<CoordinateFrameTag>> {
        self.parent_id
    }
    pub fn set_parent_id(&mut self, parent_id: Option<ObjectId<CoordinateFrameTag>>) {
        self.parent_id = parent_id;
    }
    pub fn kind(&self) -> &ConceptId {
        &self.kind
    }
    pub const fn transform(&self) -> Option<&[ExactNumber; 16]> {
        self.transform.as_ref()
    }
    pub const fn uncertainty(&self) -> Rational {
        self.uncertainty
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelSpec {
    concept: ConceptId,
    coordinate_frame_id: Option<ObjectId<CoordinateFrameTag>>,
    source_keys: Vec<SourceKey>,
}

impl ChannelSpec {
    pub fn new(concept: ConceptId) -> Self {
        Self {
            concept,
            coordinate_frame_id: None,
            source_keys: Vec::new(),
        }
    }

    pub fn with_coordinate_frame(mut self, id: ObjectId<CoordinateFrameTag>) -> Self {
        self.coordinate_frame_id = Some(id);
        self
    }

    pub fn with_source_key(mut self, key: SourceKey) -> Self {
        self.source_keys.push(key);
        self
    }

    pub fn concept(&self) -> &ConceptId {
        &self.concept
    }
    pub const fn coordinate_frame_id(&self) -> Option<ObjectId<CoordinateFrameTag>> {
        self.coordinate_frame_id
    }

    pub fn source_keys(&self) -> &[SourceKey] {
        &self.source_keys
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReferenceKind {
    Absolute,
    Common,
    Differential,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelBasis {
    id: ObjectId<ChannelBasisTag>,
    channels: Vec<ChannelSpec>,
    reference: ReferenceKind,
}

impl ChannelBasis {
    pub fn new(
        id: ObjectId<ChannelBasisTag>,
        channels: Vec<ChannelSpec>,
        reference: ReferenceKind,
    ) -> Self {
        Self {
            id,
            channels,
            reference,
        }
    }

    pub const fn id(&self) -> ObjectId<ChannelBasisTag> {
        self.id
    }
    pub fn channels(&self) -> &[ChannelSpec] {
        &self.channels
    }
    pub const fn reference(&self) -> ReferenceKind {
        self.reference
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Calibration {
    scale: Rational,
    offset: Rational,
    unit: ConceptId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CalibrationError {
    ZeroScale,
}

impl Calibration {
    pub fn new(
        scale: Rational,
        offset: Rational,
        unit: ConceptId,
    ) -> Result<Self, CalibrationError> {
        if scale.is_zero() {
            return Err(CalibrationError::ZeroScale);
        }
        Ok(Self {
            scale,
            offset,
            unit,
        })
    }

    pub const fn scale(&self) -> Rational {
        self.scale
    }
    pub const fn offset(&self) -> Rational {
        self.offset
    }
    pub fn unit(&self) -> &ConceptId {
        &self.unit
    }
}

impl fmt::Display for CalibrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("calibration scale must be nonzero")
    }
}
