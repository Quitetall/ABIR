use core::fmt;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Rational {
    numerator: i128,
    denominator: i128,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RationalError {
    ZeroDenominator,
    DenominatorOutOfRange,
}

impl Rational {
    pub fn new(numerator: i128, denominator: i128) -> Result<Self, RationalError> {
        if denominator == 0 {
            return Err(RationalError::ZeroDenominator);
        }
        if denominator == i128::MIN {
            return Err(RationalError::DenominatorOutOfRange);
        }
        if numerator == 0 {
            return Ok(Self {
                numerator: 0,
                denominator: 1,
            });
        }

        let divisor = gcd(numerator.unsigned_abs(), denominator.unsigned_abs()) as i128;
        let mut numerator = numerator / divisor;
        let mut denominator = denominator / divisor;
        if denominator < 0 {
            numerator = numerator
                .checked_neg()
                .ok_or(RationalError::DenominatorOutOfRange)?;
            denominator = -denominator;
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }

    pub const fn parts(self) -> (i128, i128) {
        (self.numerator, self.denominator)
    }
}

impl fmt::Display for Rational {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.numerator, self.denominator)
    }
}

impl fmt::Display for RationalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroDenominator => f.write_str("rational denominator is zero"),
            Self::DenominatorOutOfRange => f.write_str("rational denominator is out of range"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ExactNumber {
    Integer(i128),
    Rational(Rational),
}

impl From<i64> for ExactNumber {
    fn from(value: i64) -> Self {
        Self::Integer(i128::from(value))
    }
}

impl From<Rational> for ExactNumber {
    fn from(value: Rational) -> Self {
        Self::Rational(value)
    }
}

impl fmt::Display for ExactNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Integer(value) => value.fmt(f),
            Self::Rational(value) => value.fmt(f),
        }
    }
}

const fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let remainder = a % b;
        a = b;
        b = remainder;
    }
    a
}
