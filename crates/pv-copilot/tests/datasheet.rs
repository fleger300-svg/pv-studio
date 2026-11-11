//! C5: datasheet extraction without network: request shape, response reading, conversion to
//! an unverified component (recorded synthetic response; no API call is made).

use pv_copilot::{build_request, parse_response, to_component, MODEL};
use pv_model::command::keys;
use pv_model::component::{presets, ComponentSource, PartKind};
use pv_model::ComponentSpec;
use serde_json::{json, Value};

fn recorded() -> (u16, String) {
    let v: Value =
        serde_json::from_str(include_str!("../../../testdata/copilot/lr7_response.json")).unwrap();
    (
        u16::try_from(v["status"].as_u64().unwrap()).unwrap(),
        v["body"].to_string(),
    )
}

/// Every object of a structured-outputs schema must be closed and list all its properties.
fn check_closed(schema: &Value, path: &str) {
    if schema["type"] == "object" {
        assert_eq!(schema["additionalProperties"], false, "{path} not closed");
        let props: Vec<&String> = schema["properties"].as_object().unwrap().keys().collect();
        let required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r.as_str().unwrap())
            .collect();
        for p in &props {
            assert!(required.contains(&p.as_str()), "{path}.{p} not required");
        }
    }
    for key in ["minimum", "maximum", "minLength", "maxLength", "multipleOf"] {
        assert!(schema.get(key).is_none(), "{path}: unsupported {key}");
    }
    if let Some(obj) = schema.as_object() {
        for (k, v) in obj {
            match v {
                Value::Object(_) => check_closed(v, &format!("{path}.{k}")),
                Value::Array(items) => items
                    .iter()
                    .for_each(|i| check_closed(i, &format!("{path}.{k}[]"))),
                _ => {}
            }
        }
    }
}

#[test]
fn the_request_sends_the_pdf_and_asks_for_schema_json() {
    let call = build_request(
        "JVBERi0xLjc=",
        "LR7-54HVH.pdf",
        Some(" LR7-54HVH-490M "),
        "fr",
    );
    let b = &call.body;
    assert_eq!(b["model"], MODEL);
    assert_eq!(b["output_config"]["format"]["type"], "json_schema");
    assert_eq!(b["fallbacks"], "default");
    assert!(
        b.get("tool_choice").is_none(),
        "forced tool use is refused on this model"
    );
    assert!(b.get("thinking").is_none(), "adaptive thinking (default)");
    let content = &b["messages"][0]["content"];
    assert_eq!(
        content[0]["source"],
        json!({ "type": "base64", "media_type": "application/pdf", "data": "JVBERi0xLjc=" })
    );
    let text = content[1]["text"].as_str().unwrap();
    assert!(text.contains("Variant hint: LR7-54HVH-490M\n"), "{text}");
    assert!(call.headers.contains(&("anthropic-version", "2026-10-21")));
    assert!(call
        .headers
        .contains(&("anthropic-beta", "server-side-fallback-2026-07-01")));
    check_closed(&b["output_config"]["format"]["schema"], "schema");
}

#[test]
fn a_recorded_answer_becomes_an_unverified_module_equal_to_the_catalogue_one() {
    let (status, body) = recorded();
    let e = parse_response(status, &body).unwrap();
    let (c, warnings) = to_component(&e, None).unwrap();
    assert_eq!(c.source, ComponentSource::AiExtracted);
    assert!(!c.verified, "AI data is never verified");
    assert_eq!(warnings.len(), 1);
    // Same data as the base catalogue's module, entered by hand from the datasheet.
    assert_eq!(c.spec, presets::lr7_54hvh_490m().spec);
}

#[test]
fn errors_are_translatable() {
    let key = |status, body: Value| parse_response(status, &body.to_string()).unwrap_err().key;
    assert_eq!(
        key(
            401,
            json!({ "type": "error", "error": { "type": "authentication_error", "message": "invalid x-api-key" } })
        ),
        keys::COPILOT_HTTP
    );
    assert_eq!(
        key(
            200,
            json!({ "stop_reason": "refusal", "stop_details": { "category": "cyber" }, "content": [] })
        ),
        keys::COPILOT_REFUSED
    );
    assert_eq!(
        key(
            200,
            json!({ "stop_reason": "max_tokens", "content": [{ "type": "text", "text": "{" }] })
        ),
        keys::COPILOT_TRUNCATED
    );
    assert_eq!(
        key(
            200,
            json!({ "stop_reason": "end_turn", "content": [{ "type": "text", "text": "not json" }] })
        ),
        keys::COPILOT_INVALID
    );

    // A module without its dimensions cannot be created.
    let (status, body) = recorded();
    let mut e = parse_response(status, &body).unwrap();
    if let Some(m) = e.module.as_mut() {
        m.width_mm = None;
        m.mass_kg = None;
    }
    let err = to_component(&e, None).unwrap_err();
    assert_eq!(err.key, keys::COPILOT_INCOMPLETE);
    assert_eq!(
        err.params.get("fields"),
        Some(&pv_model::ErrorParam::Text("widthMm, massKg".into()))
    );
}

#[test]
fn parts_never_get_invented_values() {
    let body = json!({
        "stop_reason": "end_turn",
        "content": [{ "type": "text", "text": json!({
            "family": "mountingPart", "manufacturer": "K2 Systems", "model": "SingleRail 36",
            "articleNo": "2004393", "label": "", "module": null,
            "part": { "kind": "rail", "unitMassKg": 3.692, "packQty": 20, "stockLengthMm": 4800,
                      "frameHeightMinMm": null, "frameHeightMaxMm": null },
            "warnings": []
        }).to_string() }]
    });
    let e = parse_response(200, &body.to_string()).unwrap();
    let (c, warnings) = to_component(&e, None).unwrap();
    assert_eq!(c.label, "K2 Systems SingleRail 36");
    let ComponentSpec::MountingPart(p) = &c.spec else {
        panic!("part")
    };
    // No cross-section values on the datasheet: not a rail until the user enters them.
    assert_eq!(p.kind, PartKind::Other);
    assert_eq!((p.pack_qty, p.unit_mass_kg), (20, 3.692));
    assert_eq!(warnings.len(), 1);
}
