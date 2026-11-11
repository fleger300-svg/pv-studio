//! Component library commands: the catalogue (search, edit, prices, price list import,
//! datasheets) and its link with the open project (component choices of an array, update
//! proposals). Library edits never enter the project journal; project changes go through
//! commands (one batch, one undo step).

use std::path::PathBuf;

use base64::Engine as _;
use pv_layout::Search;
use pv_library::import::{ImportReport, ImportRequest};
use pv_library::snapshot::{accept_command, UpdateProposal};
use pv_library::{ChangeAuthor, ComponentSummary, LibraryEntry, RevisionInfo, SearchQuery};
use pv_model::command::keys;
use pv_model::component::{
    check_array_components, ComponentFamily, ComponentSource, Descriptions, ModuleSpec, PartKind,
    PartSpec, PriceEntry,
};
use pv_model::{
    ArrayId, BlobId, Command, CommandError, Component, ComponentId, ComponentSpec, Project,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::commands::{finish, ApplyResult};
use crate::state::AppState;

fn failure(e: impl std::fmt::Display) -> CommandError {
    CommandError::new(keys::STORE_FAILURE).with_text("detail", e.to_string())
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// A component with its history and prices.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ComponentDetail {
    pub entry: LibraryEntry,
    pub revisions: Vec<RevisionInfo>,
    pub prices: Vec<PriceEntry>,
}

/// A candidate for an array, with what makes it incompatible (empty: compatible).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    pub summary: ComponentSummary,
    pub issues: Vec<CommandError>,
}

/// Modules and systems an array can use.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArrayChoices {
    pub modules: Vec<Choice>,
    pub systems: Vec<Choice>,
}

/// A file chosen by the user, base64-encoded.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PickedFile {
    pub name: String,
    pub base64: String,
}

/// Kinds of files the library reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    PriceList,
    Datasheet,
}

/// Asks for a file (native dialog), or takes `path` (automation).
fn pick(
    app: &AppHandle,
    kind: FileKind,
    path: Option<String>,
) -> Result<Option<PathBuf>, CommandError> {
    if let Some(p) = path {
        return Ok(Some(PathBuf::from(p)));
    }
    let dialog = app.dialog().file();
    let dialog = match kind {
        FileKind::PriceList => dialog.add_filter("Excel", &["xlsx"]),
        FileKind::Datasheet => dialog.add_filter("PDF", &["pdf"]),
    };
    match dialog.blocking_pick_file() {
        Some(f) => Ok(Some(f.into_path().map_err(failure)?)),
        None => Ok(None),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn library_search(
    state: State<'_, AppState>,
    query: SearchQuery,
) -> Result<Vec<ComponentSummary>, CommandError> {
    state.with_library(|lib| lib.search(&query).map_err(CommandError::from))
}

#[tauri::command]
#[specta::specta]
pub async fn library_component(
    state: State<'_, AppState>,
    id: ComponentId,
) -> Result<ComponentDetail, CommandError> {
    state.with_library(|lib| {
        let entry = lib.get(id)?.ok_or_else(|| {
            CommandError::new(keys::COMPONENT_UNKNOWN).with_text("id", id.to_string())
        })?;
        Ok(ComponentDetail {
            entry,
            revisions: lib.revisions(id)?,
            prices: lib.prices(id)?,
        })
    })
}

/// A new, unsaved component of `family` with neutral values (a mounting system is created
/// by duplicating an existing one: it needs its parts).
#[tauri::command]
#[specta::specta]
pub async fn library_new_component(family: ComponentFamily) -> Result<Component, CommandError> {
    let spec = match family {
        ComponentFamily::Module => ComponentSpec::Module(ModuleSpec {
            length_m: 1.722,
            width_m: 1.134,
            thickness_m: 0.030,
            mass_kg: 21.0,
            electrical: None,
        }),
        ComponentFamily::MountingPart => ComponentSpec::MountingPart(PartSpec {
            unit_mass_kg: 0.0,
            pack_qty: 1,
            kind: PartKind::Other,
        }),
        ComponentFamily::MountingSystem => {
            return Err(CommandError::new(keys::COMPONENT_WRONG_FAMILY)
                .with_text("component", "")
                .with_text("expected", "module"))
        }
    };
    Ok(Component {
        id: ComponentId::new(),
        revision: 1,
        manufacturer: String::new(),
        model: String::new(),
        article_no: String::new(),
        label: String::new(),
        descriptions: Descriptions::default(),
        source: ComponentSource::Manual,
        verified: false,
        datasheet: None,
        spec,
    })
}

/// A copy of `id` under a new id (unsaved).
#[tauri::command]
#[specta::specta]
pub async fn library_duplicate(
    state: State<'_, AppState>,
    id: ComponentId,
) -> Result<Component, CommandError> {
    state.with_library(|lib| {
        let mut c = lib
            .get(id)?
            .ok_or_else(|| {
                CommandError::new(keys::COMPONENT_UNKNOWN).with_text("id", id.to_string())
            })?
            .component;
        c.id = ComponentId::new();
        c.revision = 1;
        c.source = ComponentSource::Manual;
        c.verified = false;
        Ok(c)
    })
}

/// Stores `component`: created when new, else a new revision (`component.revision` is the
/// revision it was edited from).
#[tauri::command]
#[specta::specta]
pub async fn library_save(
    state: State<'_, AppState>,
    component: Component,
    note: String,
) -> Result<Component, CommandError> {
    state.with_library(|lib| {
        let saved = if lib.get(component.id)?.is_some() {
            lib.update(component, ChangeAuthor::User, &note)?
        } else {
            lib.create(component, ChangeAuthor::User, &note)?
        };
        Ok(saved)
    })
}

#[tauri::command]
#[specta::specta]
pub async fn library_archive(
    state: State<'_, AppState>,
    id: ComponentId,
    archived: bool,
) -> Result<(), CommandError> {
    state.with_library(|lib| lib.set_archived(id, archived).map_err(CommandError::from))
}

#[tauri::command]
#[specta::specta]
pub async fn library_add_price(
    state: State<'_, AppState>,
    price: PriceEntry,
) -> Result<Vec<PriceEntry>, CommandError> {
    state.with_library(|lib| {
        lib.add_price(&price)?;
        Ok(lib.prices(price.component)?)
    })
}

#[tauri::command]
#[specta::specta]
pub async fn library_preferred_supplier(
    state: State<'_, AppState>,
) -> Result<Option<String>, CommandError> {
    state.with_library(|lib| lib.preferred_supplier().map_err(CommandError::from))
}

#[tauri::command]
#[specta::specta]
pub async fn library_set_preferred_supplier(
    state: State<'_, AppState>,
    name: Option<String>,
) -> Result<(), CommandError> {
    state.with_library(|lib| {
        lib.set_preferred_supplier(name.as_deref())
            .map_err(CommandError::from)
    })
}

/// Reads a file chosen by the user (native dialog; `path` for automation); `None` when
/// cancelled. Price lists are parsed by the front end (exceljs worker).
#[tauri::command]
#[specta::specta]
pub async fn library_pick_file(
    app: AppHandle,
    kind: FileKind,
    path: Option<String>,
) -> Result<Option<PickedFile>, CommandError> {
    let Some(path) = pick(&app, kind, path)? else {
        return Ok(None);
    };
    let bytes = std::fs::read(&path).map_err(failure)?;
    Ok(Some(PickedFile {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        base64: b64(&bytes),
    }))
}

/// Previews (`apply = false`) or imports a price list.
#[tauri::command]
#[specta::specta]
pub async fn library_import(
    state: State<'_, AppState>,
    request: ImportRequest,
    apply: bool,
) -> Result<ImportReport, CommandError> {
    state.with_library(|lib| {
        let report = if apply {
            lib.import_prices(&request)?
        } else {
            lib.preview_import(&request)?
        };
        Ok(report)
    })
}

/// Attaches a datasheet (PDF) to a component: a new revision. `None` when cancelled.
#[tauri::command]
#[specta::specta]
pub async fn library_attach_datasheet(
    app: AppHandle,
    state: State<'_, AppState>,
    id: ComponentId,
    path: Option<String>,
) -> Result<Option<Component>, CommandError> {
    let Some(path) = pick(&app, FileKind::Datasheet, path)? else {
        return Ok(None);
    };
    let bytes = std::fs::read(&path).map_err(failure)?;
    state.with_library(|lib| {
        let blob = lib.put_blob(&bytes, "application/pdf")?;
        let mut c = lib
            .get(id)?
            .ok_or_else(|| {
                CommandError::new(keys::COMPONENT_UNKNOWN).with_text("id", id.to_string())
            })?
            .component;
        c.datasheet = Some(blob);
        Ok(Some(lib.update(c, ChangeAuthor::User, "datasheet")?))
    })
}

/// A stored datasheet, base64-encoded.
#[tauri::command]
#[specta::specta]
pub async fn library_datasheet(
    state: State<'_, AppState>,
    id: BlobId,
) -> Result<Option<String>, CommandError> {
    state.with_library(|lib| Ok(lib.blob(id)?.map(|b| b64(&b.bytes))))
}

/// Modules and systems of the library for `array_id`, with their incompatibilities (the
/// array's current module is used to judge systems).
#[tauri::command]
#[specta::specta]
pub async fn array_choices(
    state: State<'_, AppState>,
    array_id: ArrayId,
) -> Result<ArrayChoices, CommandError> {
    state.with(|store| {
        let project = store.project();
        let array = project
            .arrays
            .iter()
            .find(|a| a.id == array_id)
            .ok_or_else(|| CommandError::new(keys::ENTITY_UNKNOWN).with_text("kind", "array"))?
            .clone();
        let support = project
            .roof_face(array.roof_face_id)
            .and_then(|f| f.support_grid.map(|g| g.kind));
        state.with_library(|lib| {
            let list = |family| {
                lib.search(&SearchQuery {
                    family: Some(family),
                    ..SearchQuery::default()
                })
                .map_err(CommandError::from)
            };
            let modules = list(ComponentFamily::Module)?
                .into_iter()
                .map(|summary| Choice {
                    summary,
                    issues: Vec::new(),
                })
                .collect();
            let current_module = match array.module {
                Some(id) => match project.component(id) {
                    Some(s) => Some(s.component.clone()),
                    None => lib.get(id)?.map(|e| e.component),
                },
                None => None,
            };
            let mut systems = Vec::new();
            for summary in list(ComponentFamily::MountingSystem)? {
                let issues = match &current_module {
                    Some(module) => {
                        let closure = lib.closure(&[summary.id], &pv_library::adoption::today())?;
                        let system = closure
                            .iter()
                            .find(|s| s.component.id == summary.id)
                            .map(|s| s.component.clone())
                            .ok_or_else(|| failure("closure"))?;
                        check_array_components(module, &system, support, closure.as_slice())
                    }
                    None => Vec::new(),
                };
                systems.push(Choice { summary, issues });
            }
            Ok(ArrayChoices { modules, systems })
        })
    })
}

/// Arrays whose module dimensions changed between `before` and `after` and that have
/// modules: their face is laid out again (in the same batch).
fn relayout(before: &Project, after: &Project) -> Result<Vec<Command>, CommandError> {
    let mut commands = Vec::new();
    for a in &after.arrays {
        let Some(old) = before.arrays.iter().find(|b| b.id == a.id) else {
            continue;
        };
        let moved =
            old.module_length_m != a.module_length_m || old.module_width_m != a.module_width_m;
        if moved && after.modules.iter().any(|m| m.array_id == a.id) {
            commands.push(pv_layout::plan_layout(after, a, Search::default())?);
        }
    }
    Ok(commands)
}

/// Applies `cmd` on a copy, then the relayouts it calls for: one batch.
fn with_relayout(project: &Project, cmd: Command) -> Result<Command, CommandError> {
    let mut preview = project.clone();
    cmd.apply(&mut preview)?;
    let mut commands = vec![cmd];
    for c in relayout(project, &preview)? {
        c.apply(&mut preview)?;
        commands.push(c);
    }
    Ok(if commands.len() == 1 {
        commands.remove(0)
    } else {
        Command::Batch { commands }
    })
}

/// Sets an array's module and system (frozen from the library when the project lacks them;
/// the face is laid out again when the module's size changes). One undo step.
#[tauri::command]
#[specta::specta]
pub async fn assign_components(
    app: AppHandle,
    state: State<'_, AppState>,
    array_id: ArrayId,
    module: Option<ComponentId>,
    mounting: Option<ComponentId>,
) -> Result<ApplyResult, CommandError> {
    state.with(|store| {
        let started = std::time::Instant::now();
        let cmd = state.complete_command(
            store.project(),
            Command::SetArrayComponents {
                array_id,
                module,
                mounting,
            },
        )?;
        let cmd = with_relayout(store.project(), cmd)?;
        let changes = store
            .apply(&cmd, pv_store::Author::User)
            .map_err(CommandError::from)?;
        finish(&app, &state, store, changes, started)
    })
}

/// Update proposals for the open project's components (library versions of today).
#[tauri::command]
#[specta::specta]
pub async fn component_updates(
    state: State<'_, AppState>,
) -> Result<Vec<UpdateProposal>, CommandError> {
    state.with(|store| {
        let components = store.project().components.clone();
        state.with_library(|lib| {
            lib.update_proposals(&components, &pv_library::adoption::today())
                .map_err(CommandError::from)
        })
    })
}

/// Accepts the proposals of `ids` (one undo step; faces of arrays whose module changed size
/// are laid out again).
#[tauri::command]
#[specta::specta]
pub async fn accept_updates(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Vec<ComponentId>,
) -> Result<ApplyResult, CommandError> {
    state.with(|store| {
        let started = std::time::Instant::now();
        let components = store.project().components.clone();
        let proposals = state.with_library(|lib| {
            lib.update_proposals(&components, &pv_library::adoption::today())
                .map_err(CommandError::from)
        })?;
        let chosen: Vec<UpdateProposal> = proposals
            .into_iter()
            .filter(|p| ids.contains(&p.component))
            .collect();
        let cmd = with_relayout(store.project(), accept_command(&chosen))?;
        let changes = store
            .apply(&cmd, pv_store::Author::User)
            .map_err(CommandError::from)?;
        finish(&app, &state, store, changes, started)
    })
}

/// A component created from its datasheet by the copilot (unverified), with the points the
/// user must check.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionResult {
    pub component: Component,
    pub warnings: Vec<String>,
}

fn network(e: impl std::fmt::Display) -> CommandError {
    CommandError::new(keys::COPILOT_NETWORK).with_text("detail", e.to_string())
}

/// Reads a datasheet (PDF) with Claude and creates the component it describes, unverified
/// (revision 1 by the copilot, the PDF attached). `None` when the user cancels the dialog.
/// The API key comes from `ANTHROPIC_API_KEY`; `ANTHROPIC_BASE_URL` overrides the endpoint
/// (tests). No lock is held during the network call.
#[tauri::command]
#[specta::specta]
pub async fn library_extract_datasheet(
    app: AppHandle,
    state: State<'_, AppState>,
    path: Option<String>,
    hint: Option<String>,
    language: String,
) -> Result<Option<ExtractionResult>, CommandError> {
    let key = crate::settings::current_key()?;
    let Some(path) = pick(&app, FileKind::Datasheet, path)? else {
        return Ok(None);
    };
    let bytes = std::fs::read(&path).map_err(failure)?;
    if bytes.len() > pv_copilot::MAX_PDF_BYTES {
        #[allow(clippy::cast_precision_loss)]
        let mb = |n: usize| (n as f64 / 1_048_576.0 * 10.0).round() / 10.0;
        return Err(CommandError::new(keys::COPILOT_TOO_LARGE)
            .with_number("size", mb(bytes.len()))
            .with_number("max", mb(pv_copilot::MAX_PDF_BYTES)));
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let call = pv_copilot::build_request(&b64(&bytes), &name, hint.as_deref(), &language);
    let client = reqwest::Client::builder()
        // A careful read of a long datasheet can take minutes.
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(network)?;
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
    let text = response.text().await.map_err(network)?;
    let extraction = pv_copilot::parse_response(status, &text)?;
    state.with_library(|lib| {
        let blob = lib.put_blob(&bytes, "application/pdf")?;
        let (component, warnings) = pv_copilot::to_component(&extraction, Some(blob))?;
        let component = lib.create(component, ChangeAuthor::Copilot, "datasheet extraction")?;
        Ok(Some(ExtractionResult {
            component,
            warnings,
        }))
    })
}
