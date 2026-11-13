//! M6: DXF plan of project A roof 1: valid structure, English layers, one polyline per
//! active module, one circle per fixation the pipeline laid out.

#![allow(dead_code)]

mod common;

use std::collections::HashMap;

use common::*;
use pv_pipeline::nodes::layout::Hardware;

#[test]
fn plan_of_project_a() {
    let p = ref_a_roof1();
    let mut g = graph_for(&p);
    let mut hardware = HashMap::new();
    let mut fixations = 0;
    for (i, a) in p.arrays.iter().enumerate() {
        let hw = g.get::<Hardware>(array(i, &p)).unwrap();
        if let Some(hw) = hw.as_ref() {
            fixations += hw.fixations.as_ref().map_or(0, |f| f.count());
            hardware.insert(a.id, hw.clone());
        }
    }
    let dxf = pv_cad::plan_dxf(&p, &hardware);
    assert!(dxf.starts_with("0\nSECTION\n2\nHEADER\n"));
    assert!(dxf.ends_with("0\nEOF\n"));
    for (layer, _) in pv_cad::LAYERS {
        assert!(dxf.contains(&format!("2\n{layer}\n")), "layer {layer}");
    }
    let active = p
        .modules
        .iter()
        .filter(|m| m.state == pv_model::ModuleState::Active)
        .count();
    let modules = dxf.matches("0\nPOLYLINE\n8\nPV_MODULE\n").count();
    assert_eq!(modules, active);
    assert_eq!(dxf.matches("0\nCIRCLE\n8\nFIXATION\n").count(), fixations);
    assert!(fixations > 0);
    assert!(dxf.matches("0\nLINE\n8\nRAIL\n").count() > 0);
    // Group codes and values alternate: an even number of lines.
    assert_eq!(dxf.lines().count() % 2, 0);
}
