use crate::TrainingError;
use abir::ValidationLimits;
use serde::Serialize;
use serde_json::Value;

/// RFC 8785-compatible JSON for ABIR training's restricted JSON domain.
///
/// Explicit key sorting keeps identity stable even if another dependency
/// enables serde_json's `preserve_order` feature for the shared build.
pub(crate) fn canonical_json<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, TrainingError> {
    let mut value = serde_json::to_value(value)?;
    canonicalize_value(&mut value, 0)?;
    Ok(serde_json::to_vec(&value)?)
}

fn canonicalize_value(value: &mut Value, depth: usize) -> Result<(), TrainingError> {
    if depth > ValidationLimits::default().max_nesting_depth {
        return Err(TrainingError::InvalidTrainingProgram);
    }
    match value {
        Value::Null | Value::Bool(_) | Value::String(_) => Ok(()),
        Value::Number(number) if number.is_i64() || number.is_u64() => Ok(()),
        Value::Number(_) => Err(TrainingError::InvalidTrainingProgram),
        Value::Array(values) => {
            for value in values {
                canonicalize_value(value, depth + 1)?;
            }
            Ok(())
        }
        Value::Object(values) => {
            let mut entries = core::mem::take(values).into_iter().collect::<Vec<_>>();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            for (key, mut value) in entries {
                canonicalize_value(&mut value, depth + 1)?;
                values.insert(key, value);
            }
            Ok(())
        }
    }
}
