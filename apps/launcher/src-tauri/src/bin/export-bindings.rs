//! Regenerates `apps/launcher/src/bindings.ts` without starting the app.
//! A separate binary (not a test) because Tauri test executables on Windows lack the
//! Common Controls v6 manifest that tauri-build embeds into bins.

fn main() {
    let path = projects_launcher_lib::default_bindings_path();
    match projects_launcher_lib::export_typescript_bindings(&path) {
        Ok(()) => println!("wrote {}", path.display()),
        Err(error) => {
            eprintln!("failed to export bindings: {error}");
            std::process::exit(1);
        }
    }
}
