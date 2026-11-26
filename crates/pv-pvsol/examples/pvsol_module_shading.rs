//! Shading per module of one imported project (what the 3-D view colours): distribution of
//! the total loss share, the most shaded modules, and the means against the simulation's
//! balance.
//! Usage: `cargo run -p pv-pvsol --release --example pvsol_module_shading -- <file.pvprj>`.

fn main() {
    let path = std::env::args().nth(1).expect("file");
    let file = pv_pvsol::read(&std::fs::read(path).expect("read")).expect("parse");
    let p = pv_pvsol::import(&file, "x").expect("import").project;
    let a = pv_solar::SimulationAssumptions::default();
    let t = std::time::Instant::now();
    let map = pv_solar::module_shading::module_shading(&p, a.albedo).expect("shading");
    let took = t.elapsed();
    let r = pv_solar::simulate(&p, &a).expect("simulate");
    let b = &r.balance;
    println!(
        "{} modules in {took:.2?} | mean horizon {:.2} % near {:.2} % | balance horizon {:.2} % near {:.2} %",
        map.modules.len(),
        map.horizon * 100.0,
        map.near * 100.0,
        b.horizon_kwh / b.nominal_kwh * 100.0,
        b.shading_kwh / b.nominal_kwh * 100.0
    );
    let mut total: Vec<(f64, u32)> = map
        .modules
        .iter()
        .map(|m| (m.horizon + m.near, m.module_id.0))
        .collect();
    total.sort_by(|x, y| x.0.total_cmp(&y.0));
    let q = |f: f64| total[((total.len() - 1) as f64 * f).round() as usize].0 * 100.0;
    println!(
        "total per module: min {:.1} p10 {:.1} median {:.1} p90 {:.1} max {:.1} %",
        q(0.0),
        q(0.1),
        q(0.5),
        q(0.9),
        q(1.0)
    );
    for (loss, id) in total.iter().rev().take(10) {
        let m = &p.modules[*id as usize];
        println!(
            "  module {id:4} array {:?} row {} col {} | {:.1} %",
            m.array_id,
            m.row,
            m.col,
            loss * 100.0
        );
    }
}
