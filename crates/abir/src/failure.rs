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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationFailure {
    code: FailureCode,
    severity: Severity,
    path: String,
    related_object: Option<[u8; 16]>,
}

impl ValidationFailure {
    pub fn error(code: FailureCode, path: impl AsRef<str>) -> Self {
        Self {
            code,
            severity: Severity::Error,
            path: path.as_ref().to_string(),
            related_object: None,
        }
    }

    pub fn with_related_object(mut self, object: [u8; 16]) -> Self {
        self.related_object = Some(object);
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
