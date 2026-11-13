//! PV Studio copilot. First capability (milestone C5): reading a component's datasheet (PDF)
//! with Claude and turning it into an **unverified** library component.
//!
//! This crate is pure: it builds the Messages API request and reads the response. The HTTP
//! call (and the API key) belong to the shell, so everything here is tested without network
//! access or cost.
//!
//! - Model `claude-opus-5-5`, structured outputs (`output_config.format`: the response is
//!   JSON matching [`extraction_schema`]); forced tool use is not available on this model.
//! - Server-side refusal fallback opted in (`fallbacks: "default"`).
//! - Values are taken as printed (mm, kg, %/K); `null` when not printed — never guessed.
//!   Uncertainties come back as warnings, shown to the user who reviews the data.

pub mod assistant;
pub mod datasheet;

pub use datasheet::{
    build_request, extraction_schema, key_check_outcome, key_check_request, parse_response,
    to_component, ApiCall, ExtractedModule, ExtractedPart, Extraction, KeyCheck, API_VERSION,
    BETA_FALLBACK, MAX_PDF_BYTES, MODEL,
};
