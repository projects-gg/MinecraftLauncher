//! Tests that read the launcher's own sources (rules the shell and the page must keep together).

/// Lines of the launcher's sources (`apps/launcher/<dir>`) with the given extensions.
pub fn launcher_sources(dir: &str, extensions: &[&str]) -> Vec<(String, Vec<String>)> {
    fn walk(dir: &std::path::Path, extensions: &[&str], out: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("launcher sources").flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, extensions, out);
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| extensions.contains(&e))
            {
                out.push(path);
            }
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/launcher")
        .join(dir);
    let mut files = Vec::new();
    walk(&root, extensions, &mut files);
    assert!(!files.is_empty(), "no sources under {}", root.display());
    files
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path).expect("read source");
            let lines = text.lines().map(str::to_owned).collect();
            (path.display().to_string(), lines)
        })
        .collect()
}
