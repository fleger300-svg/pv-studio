//! Type-erased node values.

use std::any::Any;
use std::sync::Arc;

use crate::error::GraphError;
use crate::key::NodeKey;

/// A node value, shared without copying.
pub type Value = Arc<dyn Any + Send + Sync>;

/// A stored result: the value, or the error that is the up-to-date result.
pub type Stored = Result<Value, GraphError>;

pub(crate) fn downcast<T: Send + Sync + 'static>(
    key: NodeKey,
    value: Value,
) -> Result<Arc<T>, GraphError> {
    value
        .downcast::<T>()
        .map_err(|_| GraphError::TypeMismatch(key))
}
