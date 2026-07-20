use crate::{
    AcquisitionTag, AtomTag, ChannelBasisTag, ChannelTag, ClockRelationTag, ClockTag,
    ConceptDictionaryTag, ConceptId, ContentId, CoordinateFrameTag, DerivationTag,
    DerivedArtifactTag, DeviceTag, EventTag, ExactNumber, FrameTransformTag, ObjectId, PatientTag,
    PolicyTag, Rational, RecordingTag, SemanticTag, SensorTag, SessionTag, SourceKey, StreamTag,
    SubjectTag,
};
use alloc::vec::Vec;
use core::fmt;

/// Common catalog payload for semantic entities that do not themselves define
/// relationships. The tag keeps otherwise identical records type-safe.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogRecord<T: SemanticTag> {
    id: ObjectId<T>,
    kind: ConceptId,
    source_keys: Vec<SourceKey>,
}

impl<T: SemanticTag> CatalogRecord<T> {
    pub fn new(id: ObjectId<T>, kind: ConceptId) -> Self {
        Self {
            id,
            kind,
            source_keys: Vec::new(),
        }
    }

    pub fn with_source_key(mut self, key: SourceKey) -> Self {
        self.source_keys.push(key);
        self
    }

    pub const fn id(&self) -> ObjectId<T> {
        self.id
    }

    pub fn kind(&self) -> &ConceptId {
        &self.kind
    }

    pub fn source_keys(&self) -> &[SourceKey] {
        &self.source_keys
    }
}

pub type Subject = CatalogRecord<SubjectTag>;
pub type Patient = CatalogRecord<PatientTag>;
pub type Session = CatalogRecord<SessionTag>;
pub type Acquisition = CatalogRecord<AcquisitionTag>;
pub type Device = CatalogRecord<DeviceTag>;
pub type Sensor = CatalogRecord<SensorTag>;
pub type Channel = CatalogRecord<ChannelTag>;
pub type ConceptDictionary = CatalogRecord<ConceptDictionaryTag>;

/// Typed edges that preserve the source hierarchy without embedding one
/// standard's containment model into catalog records.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SourceRelationship {
    PatientSubject {
        patient_id: ObjectId<PatientTag>,
        subject_id: ObjectId<SubjectTag>,
    },
    SessionSubject {
        session_id: ObjectId<SessionTag>,
        subject_id: ObjectId<SubjectTag>,
    },
    SessionPatient {
        session_id: ObjectId<SessionTag>,
        patient_id: ObjectId<PatientTag>,
    },
    AcquisitionSession {
        acquisition_id: ObjectId<AcquisitionTag>,
        session_id: ObjectId<SessionTag>,
    },
    AcquisitionDevice {
        acquisition_id: ObjectId<AcquisitionTag>,
        device_id: ObjectId<DeviceTag>,
    },
    DeviceSensor {
        device_id: ObjectId<DeviceTag>,
        sensor_id: ObjectId<SensorTag>,
    },
    SensorChannel {
        sensor_id: ObjectId<SensorTag>,
        channel_id: ObjectId<ChannelTag>,
    },
    AcquisitionRecording {
        acquisition_id: ObjectId<AcquisitionTag>,
        recording_id: ObjectId<RecordingTag>,
    },
    ChannelBasisMember {
        channel_id: ObjectId<ChannelTag>,
        basis_id: ObjectId<ChannelBasisTag>,
        position: u32,
    },
}

/// Exact relationship between two clocks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClockRelation {
    id: ObjectId<ClockRelationTag>,
    from_clock_id: ObjectId<ClockTag>,
    to_clock_id: ObjectId<ClockTag>,
    offset: Rational,
    rate: Rational,
    uncertainty: Rational,
    method: ConceptId,
    validity_start: Rational,
    validity_end: Option<Rational>,
    provenance: ContentId,
}

impl ClockRelation {
    // All fields are mandatory semantic claims; a parameter object would only
    // move, rather than reduce, the call-site obligation to name each value.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: ObjectId<ClockRelationTag>,
        from_clock_id: ObjectId<ClockTag>,
        to_clock_id: ObjectId<ClockTag>,
        offset: Rational,
        rate: Rational,
        uncertainty: Rational,
        method: ConceptId,
        validity_start: Rational,
        validity_end: Option<Rational>,
        provenance: ContentId,
    ) -> Self {
        Self {
            id,
            from_clock_id,
            to_clock_id,
            offset,
            rate,
            uncertainty,
            method,
            validity_start,
            validity_end,
            provenance,
        }
    }

    pub const fn id(&self) -> ObjectId<ClockRelationTag> {
        self.id
    }
    pub const fn from_clock_id(&self) -> ObjectId<ClockTag> {
        self.from_clock_id
    }
    pub const fn to_clock_id(&self) -> ObjectId<ClockTag> {
        self.to_clock_id
    }
    pub const fn offset(&self) -> Rational {
        self.offset
    }
    pub const fn rate(&self) -> Rational {
        self.rate
    }
    pub const fn uncertainty(&self) -> Rational {
        self.uncertainty
    }
    pub fn method(&self) -> &ConceptId {
        &self.method
    }
    pub const fn validity_start(&self) -> Rational {
        self.validity_start
    }
    pub const fn validity_end(&self) -> Option<Rational> {
        self.validity_end
    }
    pub const fn provenance(&self) -> ContentId {
        self.provenance
    }
}

/// Exact homogeneous transform between coordinate frames.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrameTransform {
    id: ObjectId<FrameTransformTag>,
    from_frame_id: ObjectId<CoordinateFrameTag>,
    to_frame_id: ObjectId<CoordinateFrameTag>,
    transform: [ExactNumber; 16],
    uncertainty: Rational,
    method: ConceptId,
}

impl FrameTransform {
    pub fn new(
        id: ObjectId<FrameTransformTag>,
        from_frame_id: ObjectId<CoordinateFrameTag>,
        to_frame_id: ObjectId<CoordinateFrameTag>,
        transform: [ExactNumber; 16],
        uncertainty: Rational,
        method: ConceptId,
    ) -> Self {
        Self {
            id,
            from_frame_id,
            to_frame_id,
            transform,
            uncertainty,
            method,
        }
    }

    pub const fn id(&self) -> ObjectId<FrameTransformTag> {
        self.id
    }
    pub const fn from_frame_id(&self) -> ObjectId<CoordinateFrameTag> {
        self.from_frame_id
    }
    pub const fn to_frame_id(&self) -> ObjectId<CoordinateFrameTag> {
        self.to_frame_id
    }
    pub const fn transform(&self) -> &[ExactNumber; 16] {
        &self.transform
    }
    pub const fn uncertainty(&self) -> Rational {
        self.uncertainty
    }
    pub fn method(&self) -> &ConceptId {
        &self.method
    }
}

/// A time interval on a named clock, with explicit timing uncertainty.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Event {
    id: ObjectId<EventTag>,
    kind: ConceptId,
    clock_id: ObjectId<ClockTag>,
    start: Rational,
    end: Rational,
    uncertainty: Rational,
}

impl Event {
    pub fn new(
        id: ObjectId<EventTag>,
        kind: ConceptId,
        clock_id: ObjectId<ClockTag>,
        start: Rational,
        end: Rational,
        uncertainty: Rational,
    ) -> Self {
        Self {
            id,
            kind,
            clock_id,
            start,
            end,
            uncertainty,
        }
    }

    pub const fn id(&self) -> ObjectId<EventTag> {
        self.id
    }
    pub fn kind(&self) -> &ConceptId {
        &self.kind
    }
    pub const fn clock_id(&self) -> ObjectId<ClockTag> {
        self.clock_id
    }
    pub const fn start(&self) -> Rational {
        self.start
    }
    pub const fn end(&self) -> Rational {
        self.end
    }
    pub const fn uncertainty(&self) -> Rational {
        self.uncertainty
    }
}

/// Logical output whose provenance is a semantic derivation relation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DerivedArtifact {
    id: ObjectId<DerivedArtifactTag>,
    content_id: ContentId,
    derivation_id: ObjectId<DerivationTag>,
}

impl DerivedArtifact {
    pub const fn new(
        id: ObjectId<DerivedArtifactTag>,
        content_id: ContentId,
        derivation_id: ObjectId<DerivationTag>,
    ) -> Self {
        Self {
            id,
            content_id,
            derivation_id,
        }
    }

    pub const fn id(&self) -> ObjectId<DerivedArtifactTag> {
        self.id
    }
    pub const fn content_id(&self) -> ContentId {
        self.content_id
    }
    pub const fn derivation_id(&self) -> ObjectId<DerivationTag> {
        self.derivation_id
    }
}

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
