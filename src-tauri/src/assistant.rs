//! Project assistant: the agent loop (Messages API with tools) and the tools it runs on the
//! open project. Reads come from the engines' results; writes go through the same commands
//! as the interface (journalled with the author `copilot`, each one undoable).
//!
//! Data sent to the API: the user's questions and the tool results (geometry, results,
//! article numbers). Never prices, client names or the project file path.

use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use pv_copilot::assistant::{
    build_turn, parse_turn, tool_results, user_message, Usage, MAX_TOOL_ROUNDS,
};
use pv_library::SearchQuery;
use pv_model::command::keys;
use pv_model::component::{ComponentFamily, ComponentSpec};
use pv_model::{
    Building, BuildingId, Command, CommandError, ComponentId, InverterId, InverterUnit,
    ModuleState, Orientation, Project, RoofKind,
};
use pv_store::Author;
use serde::Serialize;
use serde_json::{json, Value};
use specta::Type;
use tauri::{AppHandle, State};

use crate::state::AppState;

/// The conversation of the session (cleared by `assistant_reset` or a new project).
fn conversation() -> &'static Mutex<Vec<Value>> {
    static C: OnceLock<Mutex<Vec<Value>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(Vec::new()))
}

/// One answer of the assistant.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AssistantReply {
    pub text: String,
    /// Tools run for this answer, in order (`create_building`, …).
    pub actions: Vec<String>,
    /// Whether the project was changed (the interface reloads it).
    pub changed: bool,
    pub usage: Usage,
    pub cost_usd: f64,
}

fn network(e: impl std::fmt::Display) -> CommandError {
    CommandError::new(keys::COPILOT_NETWORK).with_text("detail", e.to_string())
}

/// Asks the assistant. Runs tools until it answers (at most `MAX_TOOL_ROUNDS` rounds).
#[tauri::command]
#[specta::specta]
pub async fn assistant_ask(
    app: AppHandle,
    state: State<'_, AppState>,
    question: String,
    language: String,
) -> Result<AssistantReply, CommandError> {
    let key = crate::settings::current_key()?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(network)?;
    let mut messages = conversation()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    messages.push(user_message(&question, &language));
    let mut usage = Usage::default();
    let mut actions = Vec::new();
    let mut changed = false;
    let mut text = String::new();
    for _ in 0..=MAX_TOOL_ROUNDS {
        let call = build_turn(&messages);
        let mut request = client
            .post(format!("{}/v1/messages", crate::settings::api_base()))
            .header("x-api-key", key.expose())
            .header("content-type", "application/json");
        for (name, value) in call.headers {
            request = request.header(name, value);
        }
        let response = request
            .body(call.body.to_string())
            .send()
            .await
            .map_err(network)?;
        let status = response.status().as_u16();
        let body = response.text().await.map_err(network)?;
        let turn = parse_turn(status, &body)?;
        usage.add(&turn.usage);
        messages.push(turn.message.clone());
        if !turn.text.is_empty() {
            text = turn.text.clone();
        }
        if turn.stop_reason != "tool_use" || turn.tool_calls.is_empty() {
            break;
        }
        let mut results = Vec::with_capacity(turn.tool_calls.len());
        for t in &turn.tool_calls {
            actions.push(t.name.clone());
            let (out, is_error) = match run_tool(&app, &state, &t.name, &t.input) {
                Ok((v, wrote)) => {
                    changed |= wrote;
                    (v, false)
                }
                Err(e) => (json!({ "error": e.key, "params": e.params }), true),
            };
            results.push((t.id.clone(), out, is_error));
        }
        messages.push(tool_results(&results));
    }
    *conversation()
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = messages;
    Ok(AssistantReply {
        text,
        actions,
        changed,
        cost_usd: usage.cost_usd(),
        usage,
    })
}

/// Starts a new conversation.
#[tauri::command]
#[specta::specta]
pub async fn assistant_reset() -> Result<(), CommandError> {
    conversation()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clear();
    Ok(())
}

/// Removes prices (never sent to the API) and long id lists from a tool result.
fn scrub(v: &mut Value) {
    match v {
        Value::Object(map) => {
            map.retain(|k, _| {
                let k = k.to_lowercase();
                !(k.contains("price")
                    || k.contains("discount")
                    || k.contains("amount")
                    || k.contains("currency")
                    || k == "moduleids"
                    || k == "module_ids")
            });
            map.values_mut().for_each(scrub);
        }
        Value::Array(a) => a.iter_mut().for_each(scrub),
        _ => {}
    }
}

fn to_value<T: Serialize>(t: &T) -> Value {
    let mut v = serde_json::to_value(t).unwrap_or(Value::Null);
    scrub(&mut v);
    v
}

fn invalid(field: &str) -> CommandError {
    CommandError::new(keys::ENTITY_INVALID).with_text("field", field)
}

fn str_arg<'a>(input: &'a Value, name: &str) -> Result<&'a str, CommandError> {
    input[name].as_str().ok_or_else(|| invalid(name))
}

fn num_arg(input: &Value, name: &str) -> Result<f64, CommandError> {
    input[name]
        .as_f64()
        .filter(|x| x.is_finite())
        .ok_or_else(|| invalid(name))
}

/// Applies `cmd` as the copilot and notifies the interface.
fn apply(
    app: &AppHandle,
    state: &AppState,
    cmd: impl FnOnce(&Project) -> Result<Command, CommandError>,
) -> Result<(), CommandError> {
    state.with(|store| {
        let started = Instant::now();
        let cmd = cmd(store.project())?;
        let changes = store
            .apply(&cmd, Author::Copilot)
            .map_err(CommandError::from)?;
        crate::commands::finish(app, state, store, changes, started).map(|_| ())
    })
}

fn project_facts(p: &Project) -> Value {
    let label = |id: Option<ComponentId>| {
        id.and_then(|id| p.component(id))
            .map(|s| s.component.label.clone())
    };
    let face_name = |id| {
        p.roof_faces
            .iter()
            .find(|f| f.id == id)
            .map(|f| f.name.clone())
    };
    json!({
        "location": p.site.location.as_ref().map(|l| json!({
            "address": l.label, "altitudeM": l.altitude_m, "latDeg": l.lat_deg, "lonDeg": l.lon_deg
        })),
        "weather": p.site.weather.as_ref().map(|w| json!({
            "source": w.source, "annualGhiKwhM2": w.annual_ghi_kwh_m2()
        })),
        "buildings": p.buildings.iter().map(|b| json!({
            "name": b.name, "sizeM": b.size_m, "eaveHeightM": b.eave_height_m,
            "ridgeHeightM": b.ridge_height_m, "roofKind": b.roof_kind,
            "siaSiteDataSet": b.site.is_some(),
        })).collect::<Vec<_>>(),
        "roofFaces": p.roof_faces.iter().map(|f| json!({
            "name": f.name, "tiltDeg": f.tilt_deg, "azimuthDeg": f.azimuth_deg,
            "widthM": f.width_m, "heightM": f.height_m,
            "supportGrid": f.support_grid.map(|g| json!({"kind": g.kind, "spacingM": g.spacing_m})),
        })).collect::<Vec<_>>(),
        "fields": p.arrays.iter().map(|a| {
            let n = p.modules.iter().filter(|m| m.array_id == a.id && m.state == ModuleState::Active).count();
            let wp = a.module.and_then(|id| p.component(id)).and_then(|s| match &s.component.spec {
                ComponentSpec::Module(m) => m.electrical.map(|e| e.pmax_w), _ => None });
            json!({
                "face": face_name(a.roof_face_id), "module": label(a.module), "mountingSystem": label(a.mounting),
                "orientation": a.orientation, "modules": n,
                "kWp": wp.map(|w| w * n as f64 / 1000.0),
            })
        }).collect::<Vec<_>>(),
        "inverters": p.wiring.inverters.iter().map(|i| json!({"name": i.name, "model": label(Some(i.component))})).collect::<Vec<_>>(),
        "strings": p.wiring.strings.len(),
    })
}

/// Runs one tool; returns its result and whether it changed the project.
fn run_tool(
    app: &AppHandle,
    state: &AppState,
    name: &str,
    input: &Value,
) -> Result<(Value, bool), CommandError> {
    match name {
        "get_project" => state.with(|s| Ok((project_facts(s.project()), false))),
        "get_statics" => {
            let c = state.results.read().unwrap_or_else(PoisonError::into_inner);
            Ok((to_value(&c.fields.values().collect::<Vec<_>>()), false))
        }
        "get_bill_of_materials" => {
            let c = state.results.read().unwrap_or_else(PoisonError::into_inner);
            Ok((to_value(&c.bom), false))
        }
        "check_wiring" => state.with(|s| Ok((to_value(&pv_electrical::check(s.project())), false))),
        "simulate_yield" => state.with(|s| {
            let r = pv_solar::simulate(s.project(), &pv_solar::SimulationAssumptions::default());
            Ok((
                match r {
                    Ok(r) => to_value(&r),
                    Err(e) => json!({ "notSimulated": e }),
                },
                false,
            ))
        }),
        "search_library" => {
            let family = match str_arg(input, "family")? {
                "module" => ComponentFamily::Module,
                "inverter" => ComponentFamily::Inverter,
                "optimizer" => ComponentFamily::Optimizer,
                "battery" => ComponentFamily::Battery,
                "mountingSystem" => ComponentFamily::MountingSystem,
                "mountingPart" => ComponentFamily::MountingPart,
                _ => ComponentFamily::Equipment,
            };
            let text = str_arg(input, "text")?.to_owned();
            let rows = state.with_library(|lib| {
                lib.search(&SearchQuery {
                    text: (!text.trim().is_empty()).then_some(text),
                    family: Some(family),
                    limit: Some(15),
                    ..SearchQuery::default()
                })
                .map_err(CommandError::from)
            })?;
            Ok((
                json!(rows
                    .iter()
                    .map(|r| json!({"id": r.id, "label": r.label, "verified": r.verified, "pmaxW": r.pmax_w}))
                    .collect::<Vec<_>>()),
                false,
            ))
        }
        "create_building" => {
            let (len, wid) = (num_arg(input, "length_m")?, num_arg(input, "width_m")?);
            let eave = num_arg(input, "eave_height_m")?;
            let pitch = num_arg(input, "pitch_deg")?;
            let az = num_arg(input, "long_side_azimuth_deg")?.to_radians();
            let (x, y) = (num_arg(input, "x_m")?, num_arg(input, "y_m")?);
            if !(1.0..=500.0).contains(&len)
                || !(1.0..=500.0).contains(&wid)
                || !(1.0..=100.0).contains(&eave)
                || !(0.0..=70.0).contains(&pitch)
            {
                return Err(invalid("dimensions"));
            }
            let name = str_arg(input, "name")?.to_owned();
            // Rectangle centred on the origin, long side along the azimuth (from north).
            let (dx, dy) = (az.sin(), az.cos());
            let (nx, ny) = (dy, -dx);
            let corner = |a: f64, b: f64| {
                [
                    a * len / 2.0 * dx + b * wid / 2.0 * nx,
                    a * len / 2.0 * dy + b * wid / 2.0 * ny,
                ]
            };
            let mut footprint = vec![
                corner(-1.0, -1.0),
                corner(1.0, -1.0),
                corner(1.0, 1.0),
                corner(-1.0, 1.0),
            ];
            // Counter-clockwise.
            let area: f64 = (0..4)
                .map(|i| {
                    let (p, q) = (footprint[i], footprint[(i + 1) % 4]);
                    p[0] * q[1] - q[0] * p[1]
                })
                .sum();
            if area < 0.0 {
                footprint.reverse();
            }
            let (kind, roof) = match str_arg(input, "roof")? {
                "gable" => (
                    RoofKind::Gable,
                    pv_roof::presets::gable(&footprint, pitch, 0.0),
                ),
                "hipped" => (
                    RoofKind::Hipped,
                    pv_roof::presets::hipped(&footprint, pitch, pitch, 0.0),
                ),
                "monopitch" => (
                    RoofKind::Monopitch,
                    pv_roof::presets::monopitch(&footprint, 0, pitch, 0.0),
                ),
                _ => (RoofKind::Flat, pv_roof::presets::flat(&footprint, 0.0)),
            };
            let building = Building {
                id: BuildingId::new(),
                name: name.clone(),
                origin: [x, y, 0.0],
                footprint,
                size_m: [len, wid],
                eave_height_m: eave,
                ridge_height_m: eave,
                roof_kind: kind,
                roof: Some(roof),
                site: None,
            };
            apply(app, state, |p| {
                pv_roof::plan_roof(p, &building).map_err(Into::into)
            })?;
            let faces: Vec<String> = state.with(|s| {
                Ok(s.project()
                    .roof_faces
                    .iter()
                    .filter(|f| f.building_id == building.id)
                    .map(|f| f.name.clone())
                    .collect())
            })?;
            Ok((json!({ "building": name, "roofFaces": faces }), true))
        }
        "lay_out_face" => {
            let face_name = str_arg(input, "face_name")?.to_lowercase();
            let orientation = if str_arg(input, "orientation")? == "landscape" {
                Orientation::Landscape
            } else {
                Orientation::Portrait
            };
            let face_id = state.with(|s| {
                s.project()
                    .roof_faces
                    .iter()
                    .find(|f| f.name.to_lowercase() == face_name)
                    .or_else(|| {
                        s.project()
                            .roof_faces
                            .iter()
                            .find(|f| f.name.to_lowercase().contains(&face_name))
                    })
                    .map(|f| f.id)
                    .ok_or_else(|| {
                        CommandError::new(keys::ENTITY_UNKNOWN).with_text("kind", "roofFace")
                    })
            })?;
            apply(app, state, |p| {
                let mut array = p
                    .arrays
                    .iter()
                    .find(|a| a.roof_face_id == face_id)
                    .cloned()
                    .map_or_else(|| crate::layout::default_array_for(state, p, face_id), Ok)?;
                array.orientation = orientation;
                crate::layout::layout_command(state, p, &array)
            })?;
            let n = state.with(|s| {
                let p = s.project();
                Ok(p.modules
                    .iter()
                    .filter(|m| {
                        m.state == ModuleState::Active
                            && p.arrays
                                .iter()
                                .any(|a| a.id == m.array_id && a.roof_face_id == face_id)
                    })
                    .count())
            })?;
            Ok((json!({ "modules": n }), true))
        }
        "set_mounting_system" => {
            let face_name = str_arg(input, "face_name")?.to_lowercase();
            let mounting: ComponentId = serde_json::from_value(input["mounting_id"].clone())
                .map_err(|_| invalid("mounting_id"))?;
            let arrays: Vec<(pv_model::ArrayId, Option<ComponentId>)> = state.with(|s| {
                let p = s.project();
                Ok(p.arrays
                    .iter()
                    .filter(|a| {
                        p.roof_faces.iter().any(|f| {
                            f.id == a.roof_face_id && f.name.to_lowercase().contains(&face_name)
                        })
                    })
                    .map(|a| (a.id, a.module))
                    .collect())
            })?;
            if arrays.is_empty() {
                return Err(CommandError::new(keys::ENTITY_UNKNOWN).with_text("kind", "array"));
            }
            for (array_id, module) in arrays {
                apply(app, state, |p| {
                    let cmd = state.complete_command(
                        p,
                        Command::SetArrayComponents {
                            array_id,
                            module,
                            mounting: Some(mounting),
                        },
                    )?;
                    crate::library::with_relayout(p, cmd)
                })?;
            }
            Ok((json!({ "ok": true }), true))
        }
        "add_inverter" => {
            let id: ComponentId = serde_json::from_value(input["inverter_id"].clone())
                .map_err(|_| invalid("inverter_id"))?;
            let count = input["count"]
                .as_u64()
                .filter(|c| (1..=20).contains(c))
                .ok_or_else(|| invalid("count"))?;
            apply(app, state, |p| {
                let mut w = p.wiring.clone();
                for _ in 0..count {
                    let n = w.inverters.len() + 1;
                    w.inverters.push(InverterUnit {
                        id: InverterId::new(),
                        name: format!("WR{n}"),
                        component: id,
                    });
                }
                state.complete_command(
                    p,
                    Command::SetWiring {
                        wiring: Box::new(w),
                    },
                )
            })?;
            Ok((json!({ "added": count }), true))
        }
        "auto_string" => {
            apply(app, state, |p| {
                Ok(Command::SetWiring {
                    wiring: Box::new(pv_electrical::auto::auto_string(p)),
                })
            })?;
            state.with(|s| Ok((to_value(&pv_electrical::check(s.project())), true)))
        }
        _ => Err(CommandError::new(keys::COPILOT_INVALID)
            .with_text("detail", format!("unknown tool {name}"))),
    }
}
