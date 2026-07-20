use crate::{
    ConceptId, ContentId, DerivationTag, ExactNumber, ObjectId, PolicyTag, ProofTag, SemanticRef,
    SourceKey,
};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Policy {
    id: ObjectId<PolicyTag>,
    parent_id: Option<ObjectId<PolicyTag>>,
    restrictions: Vec<ConceptId>,
}

impl Policy {
    pub fn new(
        id: ObjectId<PolicyTag>,
        parent_id: Option<ObjectId<PolicyTag>>,
        mut restrictions: Vec<ConceptId>,
    ) -> Self {
        restrictions.sort();
        restrictions.dedup();
        Self {
            id,
            parent_id,
            restrictions,
        }
    }

    pub const fn id(&self) -> ObjectId<PolicyTag> {
        self.id
    }
    pub const fn parent_id(&self) -> Option<ObjectId<PolicyTag>> {
        self.parent_id
    }
    pub fn restrictions(&self) -> &[ConceptId] {
        &self.restrictions
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Proof {
    id: ObjectId<ProofTag>,
    kind: ConceptId,
    subject: SemanticRef,
    payload: ContentId,
}

impl Proof {
    pub fn new(
        id: ObjectId<ProofTag>,
        kind: ConceptId,
        subject: SemanticRef,
        payload: ContentId,
    ) -> Self {
        Self {
            id,
            kind,
            subject,
            payload,
        }
    }

    pub const fn id(&self) -> ObjectId<ProofTag> {
        self.id
    }
    pub fn kind(&self) -> &ConceptId {
        &self.kind
    }
    pub const fn subject(&self) -> SemanticRef {
        self.subject
    }
    pub const fn payload(&self) -> ContentId {
        self.payload
    }
    pub fn satisfies(&self, required_kind: &ConceptId, subject: SemanticRef) -> bool {
        self.kind == *required_kind && self.subject == subject
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Derivation {
    id: ObjectId<DerivationTag>,
    operation: ConceptId,
    inputs: Vec<SemanticRef>,
    outputs: Vec<SemanticRef>,
}

impl Derivation {
    pub fn new(
        id: ObjectId<DerivationTag>,
        operation: ConceptId,
        inputs: Vec<SemanticRef>,
        outputs: Vec<SemanticRef>,
    ) -> Self {
        Self {
            id,
            operation,
            inputs,
            outputs,
        }
    }

    pub const fn id(&self) -> ObjectId<DerivationTag> {
        self.id
    }
    pub fn operation(&self) -> &ConceptId {
        &self.operation
    }
    pub fn inputs(&self) -> &[SemanticRef] {
        &self.inputs
    }
    pub fn outputs(&self) -> &[SemanticRef] {
        &self.outputs
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionRecord {
    operation: ConceptId,
    implementation: String,
    hardware: Option<String>,
}

impl ExecutionRecord {
    pub fn new(operation: ConceptId, implementation: impl AsRef<str>) -> Self {
        Self {
            operation,
            implementation: implementation.as_ref().to_string(),
            hardware: None,
        }
    }

    pub fn with_hardware(mut self, hardware: impl AsRef<str>) -> Self {
        self.hardware = Some(hardware.as_ref().to_string());
        self
    }

    pub fn operation(&self) -> &ConceptId {
        &self.operation
    }
    pub fn implementation(&self) -> &str {
        &self.implementation
    }
    pub fn hardware(&self) -> Option<&str> {
        self.hardware.as_deref()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FidelityKind {
    Exact,
    Bounded,
    Transformed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fidelity {
    subject: SemanticRef,
    kind: FidelityKind,
    metric: Option<ConceptId>,
    bound: Option<ExactNumber>,
}

impl Fidelity {
    pub fn new(
        subject: SemanticRef,
        kind: FidelityKind,
        metric: Option<ConceptId>,
        bound: Option<ExactNumber>,
    ) -> Self {
        Self {
            subject,
            kind,
            metric,
            bound,
        }
    }

    pub const fn subject(&self) -> SemanticRef {
        self.subject
    }
    pub const fn kind(&self) -> FidelityKind {
        self.kind
    }
    pub fn metric(&self) -> Option<&ConceptId> {
        self.metric.as_ref()
    }
    pub const fn bound(&self) -> Option<ExactNumber> {
        self.bound
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceCapsule {
    source: SourceKey,
    content_id: ContentId,
    media_type: Option<String>,
}

impl SourceCapsule {
    pub fn new(
        source: SourceKey,
        content_id: ContentId,
        media_type: Option<impl AsRef<str>>,
    ) -> Self {
        Self {
            source,
            content_id,
            media_type: media_type.map(|value| value.as_ref().to_string()),
        }
    }

    pub fn source(&self) -> &SourceKey {
        &self.source
    }
    pub const fn content_id(&self) -> ContentId {
        self.content_id
    }
    pub fn media_type(&self) -> Option<&str> {
        self.media_type.as_deref()
    }
}
