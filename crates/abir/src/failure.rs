use crate::{ConceptId, ContentId, SemanticRef};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureCode {
    DuplicateId,
    DanglingReference,
    InvalidExactNumber,
    InvalidShapeOrExtent,
    PayloadMismatch,
    UnresolvedClock,
    UnresolvedCoordinateFrame,
    InvalidCalibration,
    ProofMisuse,
    PolicyRelaxation,
    NonfiniteMetadata,
    StructuralLimit,
    UnsupportedSemanticExtension,
}

impl FailureCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DuplicateId => "ABIR-E001",
            Self::DanglingReference => "ABIR-E002",
            Self::InvalidExactNumber => "ABIR-E003",
            Self::InvalidShapeOrExtent => "ABIR-E004",
            Self::PayloadMismatch => "ABIR-E005",
            Self::UnresolvedClock => "ABIR-E006",
            Self::UnresolvedCoordinateFrame => "ABIR-E007",
            Self::InvalidCalibration => "ABIR-E008",
            Self::ProofMisuse => "ABIR-E009",
            Self::PolicyRelaxation => "ABIR-E010",
            Self::NonfiniteMetadata => "ABIR-E011",
            Self::StructuralLimit => "ABIR-E012",
            Self::UnsupportedSemanticExtension => "ABIR-E013",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryClass {
    Never,
    Immediate,
    AfterCorrection,
    Transient,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FailureOrigin {
    NamespaceCode { namespace: String, code: String },
    ConceptId(ConceptId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationFailure {
    code: FailureCode,
    severity: Severity,
    path: String,
    related_object: Option<[u8; 16]>,
    origin: FailureOrigin,
    retry_class: RetryClass,
    affected_scope: Option<SemanticRef>,
    evidence: Vec<ContentId>,
}

impl ValidationFailure {
    pub fn error(code: FailureCode, path: impl AsRef<str>) -> Self {
        Self {
            code,
            severity: Severity::Error,
            path: path.as_ref().to_string(),
            related_object: None,
            origin: FailureOrigin::NamespaceCode {
                namespace: "abir".to_string(),
                code: "validation".to_string(),
            },
            retry_class: RetryClass::AfterCorrection,
            affected_scope: None,
            evidence: Vec::new(),
        }
    }

    pub fn with_related_object(mut self, object: [u8; 16]) -> Self {
        self.related_object = Some(object);
        self
    }

    pub fn with_origin(mut self, origin: FailureOrigin) -> Self {
        self.origin = origin;
        self
    }

    pub fn with_retry_class(mut self, retry_class: RetryClass) -> Self {
        self.retry_class = retry_class;
        self
    }

    pub fn with_affected_scope(mut self, scope: SemanticRef) -> Self {
        self.affected_scope = Some(scope);
        self
    }

    pub fn with_evidence(mut self, evidence: Vec<ContentId>) -> Self {
        self.evidence = evidence;
        self
    }

    pub const fn code(&self) -> &'static str {
        self.code.as_str()
    }

    pub const fn failure_code(&self) -> FailureCode {
        self.code
    }

    pub const fn severity(&self) -> Severity {
        self.severity
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub const fn related_object(&self) -> Option<[u8; 16]> {
        self.related_object
    }

    pub fn origin(&self) -> &FailureOrigin {
        &self.origin
    }

    pub const fn retry_class(&self) -> RetryClass {
        self.retry_class
    }

    pub const fn affected_scope(&self) -> Option<SemanticRef> {
        self.affected_scope
    }

    pub fn evidence(&self) -> &[ContentId] {
        &self.evidence
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationReport {
    failures: Vec<ValidationFailure>,
}

impl ValidationReport {
    pub fn new(failure: ValidationFailure) -> Self {
        Self {
            failures: alloc::vec![failure],
        }
    }

    pub fn push(&mut self, failure: ValidationFailure) {
        self.failures.push(failure);
    }

    pub fn failures(&self) -> &[ValidationFailure] {
        &self.failures
    }

    pub fn len(&self) -> usize {
        self.failures.len()
    }

    pub fn is_empty(&self) -> bool {
        self.failures.is_empty()
    }
}
