//! Regenerates the TypeScript bindings without launching the app (`pnpm bindings`).

fn main() {
    pv_studio_lib::export_bindings(&pv_studio_lib::specta_builder())
        .expect("failed to export TypeScript bindings");
    println!("bindings written to {}", pv_studio_lib::BINDINGS_PATH);
}
