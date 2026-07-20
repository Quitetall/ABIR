use crate::Rational;
use alloc::vec::Vec;
use core::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimeSegment {
    start: Rational,
    rate: Rational,
    samples: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimeError {
    NonPositiveRate,
    EmptySegment,
    EmptyAxis,
    SampleCountOverflow,
}

impl TimeSegment {
    pub fn new(start: Rational, rate: Rational, samples: u64) -> Result<Self, TimeError> {
        if !rate.is_positive() {
            return Err(TimeError::NonPositiveRate);
        }
        if samples == 0 {
            return Err(TimeError::EmptySegment);
        }
        Ok(Self {
            start,
            rate,
            samples,
        })
    }

    pub const fn start(self) -> Rational {
        self.start
    }

    pub const fn rate(self) -> Rational {
        self.rate
    }

    pub const fn samples(self) -> u64 {
        self.samples
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TimeAxis {
    Regular(TimeSegment),
    Piecewise(Vec<TimeSegment>),
    Explicit {
        timestamps: crate::ContentId,
        count: u64,
    },
}

impl TimeAxis {
    pub fn sample_count(&self) -> Result<u64, TimeError> {
        match self {
            Self::Regular(segment) => Ok(segment.samples),
            Self::Piecewise(segments) => {
                if segments.is_empty() {
                    return Err(TimeError::EmptyAxis);
                }
                segments.iter().try_fold(0_u64, |total, segment| {
                    total
                        .checked_add(segment.samples)
                        .ok_or(TimeError::SampleCountOverflow)
                })
            }
            Self::Explicit { count: 0, .. } => Err(TimeError::EmptyAxis),
            Self::Explicit { count, .. } => Ok(*count),
        }
    }
}

impl fmt::Display for TimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonPositiveRate => f.write_str("sample rate must be positive"),
            Self::EmptySegment => f.write_str("time segment must contain samples"),
            Self::EmptyAxis => f.write_str("time axis must not be empty"),
            Self::SampleCountOverflow => f.write_str("time-axis sample count overflow"),
        }
    }
}
