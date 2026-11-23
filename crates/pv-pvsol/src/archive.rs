//! The `.pvprj` container: a zip archive. Entry names use `\` or `/` depending on the PV*SOL
//! release; they are normalised to `/`. Only the entries the reader needs are loaded (the
//! archive also holds screenshots, textures and map images).

use std::collections::BTreeMap;
use std::io::{Cursor, Read};

use crate::ReadError;

/// Entries of a `.pvprj` loaded into memory, by normalised name.
#[derive(Debug, Clone, Default)]
pub struct Archive {
    entries: BTreeMap<String, Vec<u8>>,
    /// Every entry name in the archive (normalised), loaded or not.
    pub names: Vec<String>,
}

/// Largest entry the reader loads (the height map is ≈ 8 MB).
const MAX_ENTRY_BYTES: u64 = 64 * 1024 * 1024;

fn wanted(name: &str) -> bool {
    matches!(
        name,
        "Project.xml"
            | "SimResults.xml"
            | "DB.json"
            | "HeightMap.bin"
            | "Visu3D/GeometrischeDaten.xml"
            | "Visu3D/Modul_Verschaltung.xml"
            | "Visu3D/ProjDaten.xml"
            | "Visu3D/MoSys.xml"
            | "Visu3D/BasisHorizont.txt"
    ) || (name.ends_with(".wbv") && !name.contains('/'))
}

impl Archive {
    /// Reads a `.pvprj` from its bytes.
    ///
    /// # Errors
    /// [`ReadError::NotAnArchive`] when the bytes are not a readable zip archive.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ReadError> {
        let bad = |e: &dyn std::fmt::Display| ReadError::NotAnArchive(e.to_string());
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| bad(&e))?;
        let mut out = Self::default();
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(|e| bad(&e))?;
            let name = entry.name().map_err(|e| bad(&e))?.replace('\\', "/");
            out.names.push(name.clone());
            if entry.is_dir() || !wanted(&name) || entry.size() > MAX_ENTRY_BYTES {
                continue;
            }
            let mut buf = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
            entry
                .by_ref()
                .take(MAX_ENTRY_BYTES)
                .read_to_end(&mut buf)
                .map_err(|e| bad(&e))?;
            out.entries.insert(name, buf);
        }
        Ok(out)
    }

    /// Builds an archive from named entries (tests, anonymised fixtures on disk).
    #[must_use]
    pub fn from_entries(entries: impl IntoIterator<Item = (String, Vec<u8>)>) -> Self {
        let entries: BTreeMap<String, Vec<u8>> = entries
            .into_iter()
            .map(|(n, b)| (n.replace('\\', "/"), b))
            .collect();
        Self {
            names: entries.keys().cloned().collect(),
            entries,
        }
    }

    /// Bytes of an entry, by normalised name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.entries.get(name).map(Vec::as_slice)
    }

    /// The weather file (`*.wbv` at the archive root), with its name.
    #[must_use]
    pub fn weather(&self) -> Option<(&str, &[u8])> {
        self.entries
            .iter()
            .find(|(n, _)| n.ends_with(".wbv"))
            .map(|(n, b)| (n.as_str(), b.as_slice()))
    }
}
