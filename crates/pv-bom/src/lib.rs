//! PV Studio bill of materials.
//!
//! Architecture (« Nomenclature (pv-bom) »): quantities and masses are exact per face and
//! grouped by article number; sale units are computed once on the project total (ADR-005);
//! prices are pieces × unit price; the tables follow the K2 material list (one tab per face,
//! a Total, the same columns).
//!
//! # Flow
//!
//! 1. [`FieldCounts`]: counters of each laid-out field (rail pieces, climbers, fixations,
//!    top rails and their modules…) with the parts of its mounting system. Built by
//!    `pv-pipeline`; this crate never reads the geometry.
//! 2. Each [`pv_model::PartRule`] turns a counter into pieces ([`rules`]); rail pieces are
//!    cut from stock bars ([`cutting`], first-fit decreasing, top and bottom rails apart as
//!    K2 does).
//! 3. [`compute`]: face tabs (each face cut alone, exact quantities) and the Total (rails cut
//!    on the whole project, offcuts reused across faces; the bars saved are reported).
//! 4. [`pricing`]: sale units and prices in integer cents, with the project's discount per
//!    manufacturer.

pub mod aggregate;
pub mod counts;
pub mod cutting;
pub mod error;
pub mod pricing;
pub mod rules;
pub mod table;

pub use aggregate::compute;
pub use counts::{FaceRef, FieldCounts, TopRail};
pub use cutting::{cut, CutPlan};
pub use error::BomError;
pub use table::{Bom, BomLine, BomTable, SavedBars};
