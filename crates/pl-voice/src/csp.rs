//! The voice page's Content Security Policy (design §6.2, §8.7).
//!
//! The noise suppressor is WebAssembly, and compiling it needs `'wasm-unsafe-eval'` in
//! `script-src` (WebAssembly only: `eval` and inline scripts stay forbidden). The launcher adds it
//! to the voice page's responses alone (the voice window's `on_web_resource_request`), so the main
//! page keeps the policy of `tauri.conf.json` exactly.

/// The source that lets a page compile WebAssembly.
pub const WASM_EVAL: &str = "'wasm-unsafe-eval'";

/// `policy` with [`WASM_EVAL`] in `script-src`; every other directive as it was. Without a
/// `script-src`, scripts follow `default-src`, so the new `script-src` takes its sources; without
/// either, scripts (and WebAssembly) are not restricted and nothing changes.
pub fn allow_wasm(policy: &str) -> String {
    let mut directives: Vec<String> = policy
        .split(';')
        .map(str::trim)
        .filter(|directive| !directive.is_empty())
        .map(str::to_owned)
        .collect();
    if let Some(script) = directives
        .iter_mut()
        .find(|directive| name(directive) == "script-src")
    {
        let sources: Vec<&str> = script.split_ascii_whitespace().skip(1).collect();
        *script = directive("script-src", &sources);
    } else if let Some(default) = directives
        .iter()
        .find(|directive| name(directive) == "default-src")
    {
        let sources: Vec<&str> = default.split_ascii_whitespace().skip(1).collect();
        let script = directive("script-src", &sources);
        directives.push(script);
    }
    directives.join("; ")
}

fn name(directive: &str) -> String {
    directive
        .split_ascii_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// `name` with `sources` and [`WASM_EVAL`] once; `'none'` goes, since it allows nothing next to
/// another source.
fn directive(name: &str, sources: &[&str]) -> String {
    let mut out = String::from(name);
    for source in sources {
        if !source.eq_ignore_ascii_case("'none'") && !source.eq_ignore_ascii_case(WASM_EVAL) {
            out.push(' ');
            out.push_str(source);
        }
    }
    out.push(' ');
    out.push_str(WASM_EVAL);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The launcher's policy (`tauri.conf.json`) as Tauri sends it, with a script hash it adds.
    const LAUNCHER: &str = "default-src 'self'; script-src 'self' 'sha256-abc='; style-src 'self' \
        'unsafe-inline'; connect-src 'self' ipc: http://ipc.localhost; worker-src 'self' blob:";

    #[test]
    fn only_script_src_changes() {
        assert_eq!(
            allow_wasm(LAUNCHER),
            "default-src 'self'; script-src 'self' 'sha256-abc=' 'wasm-unsafe-eval'; style-src \
             'self' 'unsafe-inline'; connect-src 'self' ipc: http://ipc.localhost; worker-src \
             'self' blob:"
        );
    }

    #[test]
    fn it_is_added_once() {
        let once = allow_wasm(LAUNCHER);
        assert_eq!(allow_wasm(&once), once);
        assert_eq!(
            allow_wasm("SCRIPT-SRC 'self' 'WASM-UNSAFE-EVAL'"),
            "script-src 'self' 'wasm-unsafe-eval'"
        );
    }

    #[test]
    fn without_script_src_it_follows_default_src() {
        assert_eq!(
            allow_wasm("default-src 'self' blob:; img-src data:"),
            "default-src 'self' blob:; img-src data:; script-src 'self' blob: 'wasm-unsafe-eval'"
        );
        assert_eq!(
            allow_wasm("default-src 'none'"),
            "default-src 'none'; script-src 'wasm-unsafe-eval'"
        );
    }

    #[test]
    fn an_unrestricted_policy_stays_unrestricted() {
        assert_eq!(allow_wasm("img-src data:;"), "img-src data:");
        assert_eq!(allow_wasm(""), "");
    }
}
