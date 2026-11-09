//! Results as the application sends them (`result:ready`).

use pv_graph::{Graph, GraphError, Instance, ResultReady};
use pv_model::command::keys;
use pv_model::{ArrayId, CommandError};
use serde::Serialize;
use specta::Type;

use crate::nodes::summary::{FieldStatics, FieldStaticsResult};

/// Outcome of a field's statics.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum FieldOutcome {
    Ok { result: FieldStaticsResult },
    Failed { error: CommandError },
}

/// Statics of one field, tagged with the command it was computed for.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FieldReport {
    pub array_id: ArrayId,
    /// Sequence number of the last command applied before the computation: a report older
    /// than the one already shown is ignored.
    pub seq: u32,
    /// Fingerprint of the inputs of this result (hex), shown in reports for traceability.
    pub input_fingerprint: String,
    pub outcome: FieldOutcome,
}

/// A graph error as a translatable error: domain failures keep their key, anything else
/// is an internal error with its description.
#[must_use]
pub fn command_error(e: &GraphError) -> CommandError {
    match e {
        GraphError::Failed { error, .. } | GraphError::Compute(error) => error.clone(),
        other => CommandError::new(keys::STATICS_INTERNAL).with_text("detail", other.to_string()),
    }
}

/// Reports of the field statics among the results a pass produced.
pub fn field_reports(graph: &mut Graph, ready: &[ResultReady], seq: u32) -> Vec<FieldReport> {
    ready
        .iter()
        .filter(|r| r.key.kind.0 == <FieldStatics as pv_graph::Kind>::NAME)
        .filter_map(|r| match r.key.instance {
            Instance::Array(array_id) => Some((array_id, r)),
            _ => None,
        })
        .map(|(array_id, r)| FieldReport {
            array_id,
            seq,
            input_fingerprint: format!("{:016x}", r.input_fingerprint),
            outcome: match graph.get::<FieldStatics>(Instance::Array(array_id)) {
                Ok(result) => FieldOutcome::Ok {
                    result: (*result).clone(),
                },
                Err(e) => FieldOutcome::Failed {
                    error: command_error(&e),
                },
            },
        })
        .collect()
}
