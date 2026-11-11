//! Mounting parts (articles of a mounting system).

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{non_negative, positive};
use crate::mounting::{Driver, RailSectionSpec};

/// Closed interval of lengths [m].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LengthRange {
    pub min_m: f64,
    pub max_m: f64,
}

impl LengthRange {
    #[must_use]
    pub fn contains(&self, x: f64) -> bool {
        (self.min_m..=self.max_m).contains(&x)
    }

    fn is_valid(&self) -> bool {
        non_negative(self.min_m) && positive(self.max_m) && self.min_m <= self.max_m
    }
}

/// Design resistance of one roof fixation [kN].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FixationCapacity {
    pub uplift_kn: f64,
    pub pressure_kn: f64,
    pub parallel_kn: f64,
}

/// What a part is, with the data the engines need for that kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum PartKind {
    /// Rail bar, cut into pieces by the bill-of-materials engine.
    Rail {
        stock_length_m: f64,
        section: RailSectionSpec,
    },
    /// Rail connector.
    Splice,
    /// Module clamp (middle or end clamp: the system's driver says which).
    Clamp {
        frame_height: LengthRange,
    },
    /// Roof fixation (standing-seam clamp, hanger bolt…); `capacity` is `None` when the
    /// manufacturer gives no design value (the fixation is then not checked).
    Fixation {
        #[specta(optional)]
        capacity: Option<FixationCapacity>,
    },
    /// Connector between crossing rails.
    Climber,
    /// Ballast block (its mass is the part's `unit_mass_kg`).
    BallastBlock,
    Other,
}

/// A mounting part.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PartSpec {
    /// Mass of one piece [kg].
    pub unit_mass_kg: f64,
    /// Pieces per sale unit (pack, bundle).
    pub pack_qty: u32,
    pub kind: PartKind,
}

impl PartSpec {
    pub(crate) fn invalid_field(&self) -> Option<&'static str> {
        if !non_negative(self.unit_mass_kg) {
            return Some("unitMassKg");
        }
        if self.pack_qty == 0 {
            return Some("packQty");
        }
        match &self.kind {
            PartKind::Rail { stock_length_m, .. } if !positive(*stock_length_m) => {
                Some("stockLengthM")
            }
            PartKind::Clamp { frame_height } if !frame_height.is_valid() => Some("frameHeight"),
            PartKind::Fixation { capacity: Some(c) }
                if ![c.uplift_kn, c.pressure_kn, c.parallel_kn]
                    .into_iter()
                    .all(positive) =>
            {
                Some("capacity")
            }
            _ => None,
        }
    }

    /// Whether a part of this kind can be counted by `driver` (a clamp cannot be counted per
    /// rail bar, a rail cannot be counted per fixation…).
    #[must_use]
    pub fn accepts(&self, driver: &Driver) -> bool {
        match driver {
            Driver::Fixation => matches!(self.kind, PartKind::Fixation { .. }),
            Driver::Climber => matches!(self.kind, PartKind::Climber),
            Driver::TopRailEnd | Driver::TopRailInnerJoint => {
                matches!(self.kind, PartKind::Clamp { .. })
            }
            Driver::BallastBlock => matches!(self.kind, PartKind::BallastBlock),
            Driver::RailJoint { .. } => matches!(self.kind, PartKind::Splice),
            Driver::RailBars { .. } => matches!(self.kind, PartKind::Rail { .. }),
            // Per-module accessories (cable clips, earthing…): any non-structural part.
            Driver::Module => matches!(self.kind, PartKind::Other | PartKind::Clamp { .. }),
        }
    }
}
