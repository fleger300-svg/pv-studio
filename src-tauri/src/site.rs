//! Project location and weather from the federal open services: address search and terrain
//! height (geo.admin.ch), MeteoSwiss Open Data station measurements → typical year. Network
//! calls only here; parsing and computations are in `pv-weather` (tested offline).
//!
//! Privacy: only the address typed by the user and the project's coordinates are sent, to
//! the federal services, when the user asks for them.

use std::time::{Duration, Instant};

use pv_model::command::keys;
use pv_model::{wgs84_to_lv95, Command, CommandError, GeoLocation, ProjectSite};
use pv_store::Author;
use pv_weather::{nearest_stations, parse_height, parse_hourly, parse_search, Place};
use tauri::{AppHandle, State};

use crate::commands::{finish, ApplyResult};
use crate::state::AppState;

const GEO_ADMIN: &str = "https://api3.geo.admin.ch/rest/services";
const METEOSWISS: &str = "https://data.geo.admin.ch/ch.meteoschweiz.ogd-smn";

fn network(e: impl std::fmt::Display) -> CommandError {
    CommandError::new(keys::SITE_NETWORK).with_text("detail", e.to_string())
}

fn client() -> Result<reqwest::Client, CommandError> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .user_agent("PV Studio")
        .build()
        .map_err(network)
}

/// Percent-encoding of a query value (RFC 3986 unreserved characters kept).
fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(b).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

async fn get_text(client: &reqwest::Client, url: &str) -> Result<Option<String>, CommandError> {
    let r = client.get(url).send().await.map_err(network)?;
    if r.status().as_u16() == 404 {
        return Ok(None);
    }
    if !r.status().is_success() {
        return Err(network(format!("HTTP {}", r.status().as_u16())));
    }
    // MeteoSwiss files are Windows-1252.
    let bytes = r.bytes().await.map_err(network)?;
    Ok(Some(match std::str::from_utf8(&bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => pv_weather::decode_cp1252(&bytes),
    }))
}

/// Swiss addresses and places matching `text` (geo.admin.ch search).
#[tauri::command]
#[specta::specta]
pub async fn site_search(text: String) -> Result<Vec<Place>, CommandError> {
    if text.trim().len() < 3 {
        return Ok(Vec::new());
    }
    let url = format!(
        "{GEO_ADMIN}/api/SearchServer?type=locations&sr=4326&limit=8&searchText={}",
        encode(text.trim())
    );
    let json = get_text(&client()?, &url).await?.unwrap_or_default();
    Ok(parse_search(&json))
}

fn apply_site(
    app: &AppHandle,
    state: &AppState,
    make: impl FnOnce(&ProjectSite) -> ProjectSite,
) -> Result<ApplyResult, CommandError> {
    state.with(|store| {
        let started = Instant::now();
        let site = make(&store.project().site);
        let changes = store
            .apply(
                &Command::SetSite {
                    site: Box::new(site),
                },
                Author::User,
            )
            .map_err(CommandError::from)?;
        finish(app, state, store, changes, started)
    })
}

/// Sets the project location to `place` with its terrain altitude (swissALTI3D). Keeps the
/// weather. One undo step.
#[tauri::command]
#[specta::specta]
pub async fn site_locate(
    app: AppHandle,
    state: State<'_, AppState>,
    place: Place,
) -> Result<ApplyResult, CommandError> {
    let (east, north) = wgs84_to_lv95(place.lat_deg, place.lon_deg);
    let url = format!("{GEO_ADMIN}/height?easting={east:.1}&northing={north:.1}&sr=2056");
    let altitude = get_text(&client()?, &url)
        .await?
        .and_then(|j| parse_height(&j))
        .ok_or_else(|| network("height"))?;
    apply_site(&app, &state, |old| ProjectSite {
        location: Some(GeoLocation {
            label: place.label.clone(),
            lat_deg: place.lat_deg,
            lon_deg: place.lon_deg,
            east_m: east,
            north_m: north,
            altitude_m: altitude,
        }),
        weather: old.weather.clone(),
        // The surroundings belong to the previous location.
        surroundings: None,
        horizon: None,
        objects: Vec::new(),
    })
}

/// Builds the typical weather year of the project location from the nearest MeteoSwiss
/// station with enough hourly radiation and temperature data (2010 onwards), and stores it
/// in the project. One undo step.
#[tauri::command]
#[specta::specta]
pub async fn site_fetch_weather(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ApplyResult, CommandError> {
    let location = state
        .with(|store| Ok(store.project().site.location.clone()))?
        .ok_or_else(|| CommandError::new(keys::SITE_NO_LOCATION))?;
    let client = client()?;
    let stations = get_text(&client, &format!("{METEOSWISS}/ogd-smn_meta_stations.csv"))
        .await?
        .map(|t| pv_weather::parse_stations(&t))
        .unwrap_or_default();
    let candidates = nearest_stations(
        &stations,
        location.lat_deg,
        location.lon_deg,
        location.altitude_m,
    );
    let mut weather = None;
    for s in candidates.iter().take(6) {
        let mut records = Vec::new();
        for file in [
            "h_historical_2010-2019",
            "h_historical_2020-2029",
            "h_recent",
        ] {
            let url = format!("{METEOSWISS}/{0}/ogd-smn_{0}_{file}.csv", s.abbr);
            if let Some(text) = get_text(&client, &url).await? {
                records.extend(parse_hourly(&text));
            }
        }
        let label = format!("station {} {}", s.abbr.to_uppercase(), s.name);
        if let Some(w) =
            pv_weather::typical::typical_year(&records, &label, s.lat_deg, s.lon_deg, s.altitude_m)
        {
            weather = Some(w);
            break;
        }
    }
    let weather = weather.ok_or_else(|| CommandError::new(keys::SITE_NO_WEATHER))?;
    apply_site(&app, &state, |old| ProjectSite {
        location: old.location.clone(),
        weather: Some(weather),
        surroundings: old.surroundings.clone(),
        horizon: old.horizon.clone(),
        objects: old.objects.clone(),
    })
}
