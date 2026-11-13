//! The project assistant (copilot v1): answers technical questions from the project's
//! computed values and builds through tools that map onto the model's commands. Pure: the
//! application runs the agent loop (`src-tauri`), executes the tools and sends the requests.
//!
//! Request shape (Messages API, manual tool loop): tools and system prompt are frozen and
//! cached (top-level `cache_control`); the full assistant `content` of every response
//! (thinking blocks included) is appended back unchanged; tool results go back in one user
//! message. Forced tool choice is not used (refused by the model); the system prompt tells
//! the model when to call which tool.

use pv_model::command::keys;
use pv_model::CommandError;
use serde::Serialize;
use serde_json::{json, Value};
use specta::Type;

use crate::datasheet::{ApiCall, API_VERSION, BETA_FALLBACK, MODEL};

/// Turns of tool calls one question may take before the loop stops.
pub const MAX_TOOL_ROUNDS: usize = 12;

/// Price of the model [USD per million tokens]: input, output, cache read, cache write.
pub const PRICE_INPUT: f64 = 4.0;
pub const PRICE_OUTPUT: f64 = 20.0;
pub const PRICE_CACHE_READ: f64 = 0.20;
pub const PRICE_CACHE_WRITE: f64 = 5.0;

const SYSTEM: &str = "You are the assistant of PV Studio, a Swiss photovoltaic design tool \
(buildings and roofs, module layout, mounting systems with SIA 261 statics and ballast, bill of \
materials, inverters and strings, yield simulation). You help a professional installer.

Rules:
- Answer in the user's language (given with each question), briefly and precisely, with units.
- Every figure about the project comes from a tool result. Never invent or estimate a project \
value: call the tool that computes it (get_project, get_statics, get_bill_of_materials, \
check_wiring, simulate_yield). If no tool gives it, say so.
- General technical knowledge (standards, good practice) is fine, flagged as general.
- To change the project, use the building tools (create_building, lay_out_face, \
set_mounting_system, add_inverter, auto_string). Each call is one step the user can undo. \
Before a large change (deleting, re-laying out a whole roof) say what you will do in one \
sentence. After building, report what changed with the tools' numbers.
- Ballast of east-west / ballasted systems is computed by the statics: read it with get_statics.
- Prices are never sent to you; do not guess them.
- Coordinates: buildings are placed in metres on the site plan (x east, y north).";

fn string_prop(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

fn number_prop(description: &str) -> Value {
    json!({ "type": "number", "description": description })
}

/// Tool definitions (order fixed: part of the cached prefix).
#[must_use]
pub fn tools() -> Value {
    let empty = json!({ "type": "object", "properties": {}, "additionalProperties": false });
    json!([
        {
            "name": "get_project",
            "description": "Summary of the open project: location and weather, buildings, roof faces (name, tilt, azimuth, covering), fields (module, mounting system, module count, kWp), inverters. Call first when you need project facts.",
            "input_schema": empty
        },
        {
            "name": "get_statics",
            "description": "Statics results per field (SIA 261/260): status, maximum utilisation, fixation spacing per wind zone, ballast blocks for ballasted systems, and errors (missing site data…).",
            "input_schema": empty
        },
        {
            "name": "get_bill_of_materials",
            "description": "Bill of materials of the project (article number, label, quantity, packs) without prices.",
            "input_schema": empty
        },
        {
            "name": "check_wiring",
            "description": "Electrical checks of inverters and strings (voltages at design temperatures, MPP range, currents, DC/AC ratio) with every finding.",
            "input_schema": empty
        },
        {
            "name": "simulate_yield",
            "description": "Hourly yield simulation over the project's typical weather year: annual AC energy, specific yield, performance ratio, monthly energy, loss balance.",
            "input_schema": empty
        },
        {
            "name": "search_library",
            "description": "Search the component library (modules, inverters, mounting systems, batteries…). Returns ids to pass to other tools.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "family": { "type": "string", "enum": ["module", "inverter", "optimizer", "battery", "mountingSystem", "mountingPart", "equipment"] },
                    "text": string_prop("Words to match in manufacturer, model or label")
                },
                "required": ["family", "text"],
                "additionalProperties": false
            },
            "strict": true
        },
        {
            "name": "create_building",
            "description": "Creates a rectangular building with a generated roof. The roof faces get names from the building name; lay them out with lay_out_face.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "name": string_prop("Building name"),
                    "length_m": number_prop("Length of the long side [m]"),
                    "width_m": number_prop("Width [m]"),
                    "eave_height_m": number_prop("Eave height above ground [m]"),
                    "roof": { "type": "string", "enum": ["gable", "hipped", "monopitch", "flat"] },
                    "pitch_deg": number_prop("Roof pitch [°] (0 for flat)"),
                    "long_side_azimuth_deg": number_prop("Direction the long side runs to [°, 90 = east-west ridge]"),
                    "x_m": number_prop("Position of the building centre on the site plan, east [m]"),
                    "y_m": number_prop("Position of the building centre on the site plan, north [m]")
                },
                "required": ["name", "length_m", "width_m", "eave_height_m", "roof", "pitch_deg", "long_side_azimuth_deg", "x_m", "y_m"],
                "additionalProperties": false
            },
            "strict": true
        },
        {
            "name": "lay_out_face",
            "description": "Lays out modules on a roof face (automatic layout with the default module and mounting system, setbacks and joints). Returns the module count.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "face_name": string_prop("Roof face name as given by get_project"),
                    "orientation": { "type": "string", "enum": ["portrait", "landscape"] }
                },
                "required": ["face_name", "orientation"],
                "additionalProperties": false
            },
            "strict": true
        },
        {
            "name": "set_mounting_system",
            "description": "Sets the mounting system (library id) of the fields of a roof face and lays the face out again.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "face_name": string_prop("Roof face name"),
                    "mounting_id": string_prop("Mounting system id from search_library")
                },
                "required": ["face_name", "mounting_id"],
                "additionalProperties": false
            },
            "strict": true
        },
        {
            "name": "add_inverter",
            "description": "Adds an inverter (library id) to the project.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "inverter_id": string_prop("Inverter id from search_library"),
                    "count": { "type": "integer", "description": "How many of this inverter (1–20)" }
                },
                "required": ["inverter_id", "count"],
                "additionalProperties": false
            },
            "strict": true
        },
        {
            "name": "auto_string",
            "description": "Builds the strings automatically on the project's inverters (locked strings kept), then returns the electrical checks.",
            "input_schema": empty
        }
    ])
}

/// One request of the loop. `messages` is the whole conversation so far (user, assistant
/// contents as returned, tool results).
#[must_use]
pub fn build_turn(messages: &[Value]) -> ApiCall {
    let body = json!({
        "model": MODEL,
        "max_tokens": 16000,
        // Chat and tool orchestration: medium depth is enough and faster.
        "output_config": { "effort": "medium" },
        "fallbacks": "default",
        "cache_control": { "type": "ephemeral" },
        "system": SYSTEM,
        "tools": tools(),
        "messages": messages,
    });
    ApiCall {
        headers: vec![
            ("anthropic-version", API_VERSION),
            ("anthropic-beta", BETA_FALLBACK),
        ],
        body,
    }
}

/// The user's question, with the language to answer in.
#[must_use]
pub fn user_message(text: &str, language: &str) -> Value {
    json!({
        "role": "user",
        "content": [{ "type": "text", "text": format!("[language: {language}]\n{text}") }]
    })
}

/// Token usage of one response.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input_tokens: f64,
    pub output_tokens: f64,
    pub cache_read_tokens: f64,
    pub cache_write_tokens: f64,
}

impl Usage {
    /// Cost in USD at the model's list prices.
    #[must_use]
    pub fn cost_usd(&self) -> f64 {
        (self.input_tokens * PRICE_INPUT
            + self.output_tokens * PRICE_OUTPUT
            + self.cache_read_tokens * PRICE_CACHE_READ
            + self.cache_write_tokens * PRICE_CACHE_WRITE)
            / 1e6
    }

    pub fn add(&mut self, o: &Usage) {
        self.input_tokens += o.input_tokens;
        self.output_tokens += o.output_tokens;
        self.cache_read_tokens += o.cache_read_tokens;
        self.cache_write_tokens += o.cache_write_tokens;
    }
}

/// A tool call of the model.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: Value,
}

/// One response of the model.
#[derive(Debug, Clone, PartialEq)]
pub struct Turn {
    /// The assistant message to append, content unchanged.
    pub message: Value,
    pub stop_reason: String,
    /// Visible text of the response.
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Usage,
}

/// Parses one Messages API response.
///
/// # Errors
/// The datasheet extraction's errors: HTTP status, refusal, truncation.
pub fn parse_turn(status: u16, body: &str) -> Result<Turn, CommandError> {
    if status != 200 {
        return Err(CommandError::new(keys::COPILOT_HTTP).with_number("status", f64::from(status)));
    }
    let v: Value = serde_json::from_str(body)
        .map_err(|e| CommandError::new(keys::COPILOT_INVALID).with_text("detail", e.to_string()))?;
    let stop_reason = v["stop_reason"].as_str().unwrap_or_default().to_owned();
    if stop_reason == "refusal" {
        return Err(CommandError::new(keys::COPILOT_REFUSED));
    }
    if stop_reason == "max_tokens" {
        return Err(CommandError::new(keys::COPILOT_TRUNCATED));
    }
    let content = v["content"].as_array().cloned().unwrap_or_default();
    let mut text = String::new();
    let mut tool_calls = Vec::new();
    for block in &content {
        match block["type"].as_str() {
            Some("text") => {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(block["text"].as_str().unwrap_or_default());
            }
            Some("tool_use") => tool_calls.push(ToolCall {
                id: block["id"].as_str().unwrap_or_default().to_owned(),
                name: block["name"].as_str().unwrap_or_default().to_owned(),
                input: block["input"].clone(),
            }),
            _ => {}
        }
    }
    let u = &v["usage"];
    let n = |k: &str| u[k].as_f64().unwrap_or(0.0);
    Ok(Turn {
        message: json!({ "role": "assistant", "content": content }),
        stop_reason,
        text,
        tool_calls,
        usage: Usage {
            input_tokens: n("input_tokens"),
            output_tokens: n("output_tokens"),
            cache_read_tokens: n("cache_read_input_tokens"),
            cache_write_tokens: n("cache_creation_input_tokens"),
        },
    })
}

/// The user message carrying the results of every tool call of a turn.
#[must_use]
pub fn tool_results(results: &[(String, Value, bool)]) -> Value {
    let blocks: Vec<Value> = results
        .iter()
        .map(|(id, content, is_error)| {
            json!({
                "type": "tool_result",
                "tool_use_id": id,
                "content": content.to_string(),
                "is_error": is_error,
            })
        })
        .collect();
    json!({ "role": "user", "content": blocks })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_is_cached_and_never_forces_a_tool() {
        let call = build_turn(&[user_message("Combien de modules ?", "fr")]);
        let b = &call.body;
        assert_eq!(b["model"], "claude-opus-5-5");
        assert_eq!(b["cache_control"]["type"], "ephemeral");
        assert!(b.get("tool_choice").is_none());
        assert!(b.get("thinking").is_none());
        let names: Vec<&str> = b["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"create_building") && names.contains(&"get_statics"));
        assert!(b["system"].as_str().unwrap().contains("Never invent"));
    }

    #[test]
    fn tool_use_turn_is_parsed_and_kept_whole() {
        let body = r#"{"content":[{"type":"thinking","thinking":"","signature":"s"},{"type":"text","text":"Je regarde."},{"type":"tool_use","id":"tu_1","name":"get_project","input":{}}],"stop_reason":"tool_use","usage":{"input_tokens":100,"output_tokens":50,"cache_read_input_tokens":4000,"cache_creation_input_tokens":0}}"#;
        let t = parse_turn(200, body).unwrap();
        assert_eq!(t.stop_reason, "tool_use");
        assert_eq!(t.tool_calls[0].name, "get_project");
        assert_eq!(
            t.message["content"].as_array().unwrap().len(),
            3,
            "thinking kept"
        );
        // 100 × 4 + 50 × 20 + 4000 × 0.2 = 2200 µ$.
        assert!((t.usage.cost_usd() - 0.0022).abs() < 1e-12);
        let r = tool_results(&[("tu_1".into(), json!({"modules": 35}), false)]);
        assert_eq!(r["content"][0]["tool_use_id"], "tu_1");
        assert_eq!(
            parse_turn(200, r#"{"stop_reason":"refusal","content":[]}"#)
                .unwrap_err()
                .key,
            keys::COPILOT_REFUSED
        );
    }
}
