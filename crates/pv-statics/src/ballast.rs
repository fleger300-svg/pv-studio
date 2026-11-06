//! Ballast of flat-roof systems: uplift and sliding (SIA 260 equilibrium, EQU).
//!
//! For each ballast position (support point, or module pair of an aerodynamic system):
//!
//! - uplift:  γ_G,stb·m·g ≥ γ_Q·K_FI·K·F_z
//! - sliding: μ·(γ_G,stb·m·g − γ_Q·K_FI·K·F_z) ≥ γ_Q·K_FI·K·F_x
//!
//! with m = own mass + ballast, F_z / F_x the characteristic uplift and horizontal wind forces
//! and K the product of the system's correction factors. Hence the required total mass is
//! m_req = γ_Q·K_FI·K/(γ_G,stb·g)·max(F_z, F_z + F_x/μ), and the ballast is rounded up to
//! whole blocks.
//!
//! Aerodynamic systems (K2 D-Dome, S-Dome…) give their wind forces per module pair from
//! wind-tunnel coefficients c_p,E / c_p,W of the east and west module:
//! F_z = q_p·A_M·cos α·|c_p,E + c_p,W|, F_x = q_p·A_M·sin α·(c_p,E − c_p,W).
//! These coefficients are manufacturer data ([`AeroBallastTable`]); this reproduces K2 Base
//! 3.2.63 to the rounding of its printed values (reference `ref-b`).

use serde::{Deserialize, Serialize};

use crate::combos::{PartialFactors, GRAVITY};
use crate::cp::RoofZone;
use crate::trace::{NormRef, Traced, Unit};
use crate::StaticsError;

pub const SIA_260_EQU: NormRef = NormRef::new("SIA 260:2013", "§4.4.3.1 (EQU)");

/// Roof surface under the ballast, with the friction coefficient from the material database.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoofSurface {
    pub membrane: RoofMembrane,
    /// Design friction coefficient μ between system and roof [-].
    pub friction_coefficient: f64,
    /// Where μ comes from (material database entry, manufacturer test…).
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoofMembrane {
    Epdm,
    Bitumen,
    Pvc,
    Tpo,
    Gravel,
    Green,
}

/// A ballast product (concrete block, gravel tray…).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BallastProduct {
    pub name: String,
    pub unit_mass_kg: f64,
}

/// Characteristic wind forces on one ballast position [kN] and its own mass [kg].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PositionActions {
    /// Uplift, ≥ 0.
    pub uplift_kn: f64,
    /// Horizontal force, ≥ 0. Zero when sliding is not checked at this position.
    pub horizontal_kn: f64,
    /// Product of the correction factors K applied to the wind forces.
    pub correction_factor: f64,
    /// Own mass of module(s) and mounting system at this position, without ballast [kg].
    pub own_mass_kg: f64,
}

/// Which equilibrium check governs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BallastCase {
    Uplift,
    Sliding,
}

/// Ballast result of one position.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BallastResult {
    pub case: BallastCase,
    /// Required total stabilising mass m_req [kg].
    pub required_mass: Traced,
    /// Required ballast max(0, m_req − m_DL) [kg].
    pub ballast_mass: Traced,
    /// Whole blocks of the ballast product.
    pub blocks: u32,
    pub block_mass_kg: f64,
}

/// Required ballast for one position, for one check.
pub fn required_ballast(
    actions: &PositionActions,
    case: BallastCase,
    friction_coefficient: f64,
    factors: &PartialFactors,
    importance_factor: f64,
    block: &BallastProduct,
) -> Result<BallastResult, StaticsError> {
    StaticsError::check_range("uplift_kn", actions.uplift_kn, 0.0, 1000.0)?;
    StaticsError::check_range("horizontal_kn", actions.horizontal_kn, 0.0, 1000.0)?;
    StaticsError::check_range("friction_coefficient", friction_coefficient, 0.05, 1.5)?;
    StaticsError::check_range("block_mass_kg", block.unit_mass_kg, 0.1, 2000.0)?;
    let gamma = factors.gamma_q * importance_factor / factors.gamma_g_stb;
    let k = actions.correction_factor;
    let demand = match case {
        BallastCase::Uplift => actions.uplift_kn,
        BallastCase::Sliding => actions.uplift_kn + actions.horizontal_kn / friction_coefficient,
    };
    let m_req = gamma * k * demand * 1000.0 / GRAVITY;
    let formula = match case {
        BallastCase::Uplift => "m_req = γ_Q·K_FI·K·F_z/(γ_G,stb·g)",
        BallastCase::Sliding => "m_req = γ_Q·K_FI·K·(F_z + F_x/μ)/(γ_G,stb·g)",
    };
    let required_mass = Traced::new("m_req", m_req, Unit::Kilogram)
        .formula(formula)
        .reference(SIA_260_EQU)
        .input("γ_Q", factors.gamma_q, Unit::Dimensionless)
        .input("K_FI", importance_factor, Unit::Dimensionless)
        .input("γ_G,stb", factors.gamma_g_stb, Unit::Dimensionless)
        .input("K", k, Unit::Dimensionless)
        .input("F_z", actions.uplift_kn, Unit::KiloNewton)
        .input("F_x", actions.horizontal_kn, Unit::KiloNewton)
        .input("μ", friction_coefficient, Unit::Dimensionless);
    let ballast = (m_req - actions.own_mass_kg).max(0.0);
    let ballast_mass = Traced::new("m_B", ballast, Unit::Kilogram)
        .formula("m_B = max(0, m_req − m_DL)")
        .input_from(&required_mass)
        .input("m_DL", actions.own_mass_kg, Unit::Kilogram);
    let blocks = blocks_for(ballast, block.unit_mass_kg);
    Ok(BallastResult {
        case,
        required_mass,
        ballast_mass,
        blocks,
        block_mass_kg: block.unit_mass_kg,
    })
}

/// Whole blocks for a ballast mass (rounded up; a tolerance of 1 g absorbs float noise).
#[must_use]
pub fn blocks_for(ballast_kg: f64, block_kg: f64) -> u32 {
    if ballast_kg <= 1e-3 {
        0
    } else {
        ((ballast_kg - 1e-3) / block_kg).ceil().max(1.0) as u32
    }
}

/// Governing result of uplift and sliding.
pub fn governing_ballast(
    actions: &PositionActions,
    surface: &RoofSurface,
    factors: &PartialFactors,
    importance_factor: f64,
    block: &BallastProduct,
) -> Result<BallastResult, StaticsError> {
    let up = required_ballast(
        actions,
        BallastCase::Uplift,
        surface.friction_coefficient,
        factors,
        importance_factor,
        block,
    )?;
    if actions.horizontal_kn <= 0.0 {
        return Ok(up);
    }
    let slide = required_ballast(
        actions,
        BallastCase::Sliding,
        surface.friction_coefficient,
        factors,
        importance_factor,
        block,
    )?;
    Ok(if slide.required_mass.value > up.required_mass.value {
        slide
    } else {
        up
    })
}

// ---------------------------------------------------------------------------------------------
// Aerodynamic east-west systems
// ---------------------------------------------------------------------------------------------

/// Wind-tunnel coefficients of one module pair for one check.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PairCoefficients {
    pub cp_east: f64,
    pub cp_west: f64,
    /// Correction factors (K2: k, k_d, k_p, k_s, k_R, k_B).
    pub k_slope: f64,
    pub k_direction: f64,
    pub k_parapet: f64,
    pub k_module_size: f64,
    pub k_row_spacing: f64,
    pub k_building_height: f64,
}

impl PairCoefficients {
    #[must_use]
    pub fn correction_factor(&self) -> f64 {
        self.k_slope
            * self.k_direction
            * self.k_parapet
            * self.k_module_size
            * self.k_row_spacing
            * self.k_building_height
    }
}

/// Coefficients of one roof zone: one set for the uplift check, one for sliding.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AeroZoneEntry {
    pub zone: RoofZone,
    pub uplift: PairCoefficients,
    pub sliding: PairCoefficients,
}

/// Manufacturer aerodynamic table of an east-west system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AeroBallastTable {
    pub manufacturer: String,
    pub product: String,
    /// Wind-tunnel report, or the report the values were back-fitted from.
    pub document: String,
    pub module_tilt_deg: f64,
    pub entries: Vec<AeroZoneEntry>,
}

/// East-west module pair of an aerodynamic system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AeroPair {
    /// Area of one module [m²].
    pub module_area_m2: f64,
    /// Mass of the two modules and their mounting system, without ballast [kg].
    pub own_mass_kg: f64,
}

/// Wind forces on one module pair [kN].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PairForces {
    pub uplift: Traced,
    pub horizontal: Traced,
}

/// F_z = q_p·A_M·cos α·|c_p,E + c_p,W| (uplift if the sum is negative), F_x = q_p·A_M·sin α·(c_p,E − c_p,W).
#[must_use]
pub fn pair_forces(qp: f64, pair: &AeroPair, tilt_deg: f64, c: &PairCoefficients) -> PairForces {
    let (sin, cos) = tilt_deg.to_radians().sin_cos();
    let a = pair.module_area_m2;
    let sum = c.cp_east + c.cp_west;
    let fz = if sum < 0.0 { qp * a * cos * -sum } else { 0.0 };
    let fx = (qp * a * sin * (c.cp_east - c.cp_west)).abs();
    let base = |t: Traced| {
        t.input("q_p", qp, Unit::KiloNewtonPerSquareMetre)
            .input("A_M", a, Unit::SquareMetre)
            .input("α", tilt_deg, Unit::Degree)
            .input("c_p,E", c.cp_east, Unit::Dimensionless)
            .input("c_p,W", c.cp_west, Unit::Dimensionless)
    };
    PairForces {
        uplift: base(
            Traced::new("F_z", fz, Unit::KiloNewton).formula("F_z = q_p·A_M·cos α·|c_p,E + c_p,W|"),
        ),
        horizontal: base(
            Traced::new("F_x", fx, Unit::KiloNewton).formula("F_x = q_p·A_M·sin α·|c_p,E − c_p,W|"),
        ),
    }
}

/// Ballast of one module pair in one zone: uplift and sliding are evaluated with their own
/// coefficient set; the larger requirement governs.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PairBallast {
    pub zone: RoofZone,
    pub uplift_forces: PairForces,
    pub sliding_forces: PairForces,
    pub uplift: BallastResult,
    pub sliding: BallastResult,
    pub governing: BallastCase,
    pub blocks: u32,
}

#[allow(clippy::too_many_arguments)]
pub fn aero_pair_ballast(
    table: &AeroBallastTable,
    zone: RoofZone,
    qp: f64,
    pair: &AeroPair,
    surface: &RoofSurface,
    factors: &PartialFactors,
    importance_factor: f64,
    block: &BallastProduct,
) -> Result<PairBallast, StaticsError> {
    let entry = table
        .entries
        .iter()
        .find(|e| e.zone == zone)
        .ok_or(StaticsError::MissingCoefficient { zone })?;
    let provenance = format!(
        "{} {} — {}",
        table.manufacturer, table.product, table.document
    );
    let uplift_forces = pair_forces(qp, pair, table.module_tilt_deg, &entry.uplift);
    let sliding_forces = pair_forces(qp, pair, table.module_tilt_deg, &entry.sliding);
    let uplift_actions = PositionActions {
        uplift_kn: uplift_forces.uplift.value,
        horizontal_kn: 0.0,
        correction_factor: entry.uplift.correction_factor(),
        own_mass_kg: pair.own_mass_kg,
    };
    let sliding_actions = PositionActions {
        uplift_kn: sliding_forces.uplift.value,
        horizontal_kn: sliding_forces.horizontal.value,
        correction_factor: entry.sliding.correction_factor(),
        own_mass_kg: pair.own_mass_kg,
    };
    let mu = surface.friction_coefficient;
    let mut uplift = required_ballast(
        &uplift_actions,
        BallastCase::Uplift,
        mu,
        factors,
        importance_factor,
        block,
    )?;
    let mut sliding = required_ballast(
        &sliding_actions,
        BallastCase::Sliding,
        mu,
        factors,
        importance_factor,
        block,
    )?;
    for r in [&mut uplift, &mut sliding] {
        r.required_mass = r
            .required_mass
            .clone()
            .assume("statics.assumption.ballast.aero_table", provenance.clone());
    }
    sliding.required_mass = sliding.required_mass.clone().assume(
        "statics.assumption.ballast.friction",
        format!("μ = {mu:.2} ({})", surface.source),
    );
    let governing = if sliding.required_mass.value > uplift.required_mass.value {
        BallastCase::Sliding
    } else {
        BallastCase::Uplift
    };
    let blocks = uplift.blocks.max(sliding.blocks);
    Ok(PairBallast {
        zone,
        uplift_forces,
        sliding_forces,
        uplift,
        sliding,
        governing,
        blocks,
    })
}

// ---------------------------------------------------------------------------------------------
// Mechanically linked positions
// ---------------------------------------------------------------------------------------------

/// Mechanical link between two ballast positions (rails, connectors).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub a: usize,
    pub b: usize,
    /// An expansion joint cuts the link: the positions no longer share ballast.
    pub crosses_expansion_joint: bool,
}

/// Ballast of a group of mechanically linked positions.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GroupBallast {
    pub positions: Vec<usize>,
    /// Σ m_req − Σ m_DL over the group, ≥ 0 [kg].
    pub ballast_mass_kg: f64,
    pub blocks: u32,
    /// Blocks per position, placed where the local deficit is (largest deficit first).
    pub blocks_per_position: Vec<(usize, u32)>,
}

fn find(parent: &mut [usize], i: usize) -> usize {
    let mut r = i;
    while parent[r] != r {
        r = parent[r];
    }
    let mut j = i;
    while parent[j] != r {
        let next = parent[j];
        parent[j] = r;
        j = next;
    }
    r
}

/// Groups linked positions (links across expansion joints are ignored) and shares the ballast
/// inside each group: a group needs Σ m_req ≤ Σ (m_DL + m_B). `required` and `own` are per
/// position [kg].
pub fn share_ballast(
    required_kg: &[f64],
    own_kg: &[f64],
    links: &[Link],
    block_kg: f64,
) -> Result<Vec<GroupBallast>, StaticsError> {
    if required_kg.len() != own_kg.len() {
        return Err(StaticsError::Unsupported {
            what: "required and own masses of different lengths".into(),
        });
    }
    let n = required_kg.len();
    let mut parent: Vec<usize> = (0..n).collect();
    for l in links.iter().filter(|l| !l.crosses_expansion_joint) {
        if l.a >= n || l.b >= n {
            return Err(StaticsError::Unsupported {
                what: format!("link {}–{} outside the positions", l.a, l.b),
            });
        }
        let (ra, rb) = (find(&mut parent, l.a), find(&mut parent, l.b));
        if ra != rb {
            parent[ra.max(rb)] = ra.min(rb);
        }
    }
    let mut groups: Vec<(usize, Vec<usize>)> = Vec::new();
    for i in 0..n {
        let r = find(&mut parent, i);
        match groups.iter_mut().find(|(root, _)| *root == r) {
            Some((_, v)) => v.push(i),
            None => groups.push((r, vec![i])),
        }
    }
    Ok(groups
        .into_iter()
        .map(|(_, positions)| {
            let deficit: f64 = positions.iter().map(|&i| required_kg[i] - own_kg[i]).sum();
            let ballast = deficit.max(0.0);
            let blocks = blocks_for(ballast, block_kg);
            // Place blocks where the local deficit is largest.
            let mut order: Vec<usize> = positions.clone();
            order.sort_by(|&a, &b| {
                (required_kg[b] - own_kg[b]).total_cmp(&(required_kg[a] - own_kg[a]))
            });
            let mut remaining = blocks;
            let mut per: Vec<(usize, u32)> = positions.iter().map(|&i| (i, 0)).collect();
            for &i in &order {
                if remaining == 0 {
                    break;
                }
                let local =
                    blocks_for((required_kg[i] - own_kg[i]).max(0.0), block_kg).min(remaining);
                if let Some(slot) = per.iter_mut().find(|(p, _)| *p == i) {
                    slot.1 = local;
                }
                remaining -= local;
            }
            // Blocks left once every local deficit is covered go to the largest deficit.
            if remaining > 0 {
                if let Some(slot) = per.iter_mut().find(|(p, _)| *p == order[0]) {
                    slot.1 += remaining;
                }
            }
            GroupBallast {
                positions,
                ballast_mass_kg: ballast,
                blocks,
                blocks_per_position: per,
            }
        })
        .collect())
}

/// Mechanical links of a laid-out field (`pv-layout` mounting blocks). Positions are the
/// structure's cells, in the same order; links across an expansion joint keep their flag so
/// that [`share_ballast`] ignores them.
#[must_use]
pub fn links_from_structure(structure: &pv_layout::Structure) -> Vec<Link> {
    structure
        .links
        .iter()
        .map(|l| Link {
            a: l.a,
            b: l.b,
            crosses_expansion_joint: l.crosses_expansion_joint,
        })
        .collect()
}

/// [`share_ballast`] over the mounting blocks of a field: one group per block.
pub fn share_ballast_on_structure(
    structure: &pv_layout::Structure,
    required_kg: &[f64],
    own_kg: &[f64],
    block_kg: f64,
) -> Result<Vec<GroupBallast>, StaticsError> {
    if required_kg.len() != structure.block_of.len() {
        return Err(StaticsError::Unsupported {
            what: format!(
                "{} masses for {} positions",
                required_kg.len(),
                structure.block_of.len()
            ),
        });
    }
    share_ballast(
        required_kg,
        own_kg,
        &links_from_structure(structure),
        block_kg,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_round_up() {
        assert_eq!(blocks_for(0.0, 11.0), 0);
        assert_eq!(blocks_for(6.7, 11.0), 1);
        assert_eq!(blocks_for(11.0, 11.0), 1);
        assert_eq!(blocks_for(11.01, 11.0), 2);
        assert_eq!(blocks_for(34.9, 11.0), 4);
    }

    #[test]
    fn sliding_needs_more_with_low_friction() {
        let a = PositionActions {
            uplift_kn: 0.2,
            horizontal_kn: 0.1,
            correction_factor: 1.0,
            own_mass_kg: 20.0,
        };
        let block = BallastProduct {
            name: "Block".into(),
            unit_mass_kg: 11.0,
        };
        let f = PartialFactors::sia_260();
        let hi = required_ballast(&a, BallastCase::Sliding, 0.7, &f, 1.0, &block).unwrap();
        let lo = required_ballast(&a, BallastCase::Sliding, 0.4, &f, 1.0, &block).unwrap();
        assert!(lo.ballast_mass.value > hi.ballast_mass.value);
    }

    #[test]
    fn linked_positions_share_and_joints_split() {
        // Position 0 needs 30 kg more than it weighs, position 1 has a 25 kg surplus.
        let required = [50.0, 20.0, 50.0];
        let own = [20.0, 45.0, 20.0];
        let links = [
            Link {
                a: 0,
                b: 1,
                crosses_expansion_joint: false,
            },
            Link {
                a: 1,
                b: 2,
                crosses_expansion_joint: true,
            },
        ];
        let groups = share_ballast(&required, &own, &links, 11.0).unwrap();
        assert_eq!(groups.len(), 2);
        let g0 = groups.iter().find(|g| g.positions.contains(&0)).unwrap();
        assert_eq!(g0.positions, vec![0, 1]);
        assert!((g0.ballast_mass_kg - 5.0).abs() < 1e-9);
        assert_eq!(g0.blocks, 1);
        let g2 = groups.iter().find(|g| g.positions.contains(&2)).unwrap();
        assert_eq!(g2.blocks, 3);
        assert_eq!(g2.blocks_per_position, vec![(2, 3)]);
    }
}
