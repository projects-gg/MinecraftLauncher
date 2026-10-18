//! The page the browser shows after Microsoft redirects back to the launcher.

use pl_lang::Lang;

/// Content-Security-Policy sent with the page: inline styles only, no scripts, no requests.
pub(crate) const PAGE_CSP: &str = "default-src 'none'; style-src 'unsafe-inline'; base-uri 'none'; \
                                   form-action 'none'; frame-ancestors 'none'";

const STYLE: &str = r#"
:root{--bg:#0b0907;--panel:#14100c;--line:rgba(232,176,64,.16);--gold:#E8B040;--ember:#E0861E;
--text:#f5eee2;--muted:#b3a58f;--faint:#7f725f;--danger:#ef6a4c}
*{box-sizing:border-box;margin:0;padding:0}
html,body{min-height:100%}
body{display:grid;place-items:center;min-height:100vh;padding:24px;color:var(--text);
background:radial-gradient(900px 520px at 50% -12%,rgba(232,176,64,.16),transparent 62%),
radial-gradient(760px 420px at 50% 112%,rgba(224,134,30,.10),transparent 60%),var(--bg);
font:16px/1.6 "Segoe UI Variable Text","Segoe UI",system-ui,-apple-system,BlinkMacSystemFont,
"Helvetica Neue",Arial,sans-serif;-webkit-font-smoothing:antialiased;text-rendering:optimizeLegibility}
.card{position:relative;width:min(448px,100%);padding:40px 36px 30px;text-align:center;
background:linear-gradient(180deg,rgba(232,176,64,.07),rgba(232,176,64,0) 38%),var(--panel);
border:1px solid var(--line);border-radius:22px;
box-shadow:0 32px 90px rgba(0,0,0,.6),inset 0 1px 0 rgba(255,236,200,.04)}
.card::before{content:"";position:absolute;top:-1px;left:22%;right:22%;height:1px;
background:linear-gradient(90deg,transparent,var(--gold),transparent)}
.brand{display:inline-flex;align-items:center;gap:10px;font-size:12px;font-weight:700;
letter-spacing:.3em;text-transform:uppercase;color:var(--gold)}
.brand i{width:7px;height:7px;border-radius:2px;transform:rotate(45deg);
background:linear-gradient(135deg,var(--gold),var(--ember));box-shadow:0 0 12px rgba(232,176,64,.7)}
.mark{width:84px;height:84px;margin:30px auto 24px;border-radius:50%;display:grid;place-items:center;
border:1px solid rgba(232,176,64,.5);
background:radial-gradient(circle at 50% 30%,rgba(232,176,64,.24),rgba(232,176,64,.03) 70%);
box-shadow:0 0 0 9px rgba(232,176,64,.05),0 0 48px rgba(232,176,64,.28)}
.mark svg{width:38px;height:38px;fill:none;stroke:var(--gold);stroke-width:2.6;
stroke-linecap:round;stroke-linejoin:round}
.fail .mark{border-color:rgba(239,106,76,.5);
background:radial-gradient(circle at 50% 30%,rgba(239,106,76,.2),rgba(239,106,76,.03) 70%);
box-shadow:0 0 0 9px rgba(239,106,76,.05),0 0 48px rgba(239,106,76,.22)}
.fail .mark svg{stroke:var(--danger)}
h1{font-size:23px;line-height:1.3;font-weight:650;letter-spacing:-.01em}
.lead{margin-top:10px;color:var(--muted);font-size:15px}
.hint{margin-top:28px;padding-top:18px;border-top:1px solid var(--line);font-size:13px;color:var(--faint)}
.hint b{color:var(--gold);font-weight:600}
@media (prefers-reduced-motion:no-preference){
.mark{animation:rise .6s cubic-bezier(.2,.9,.25,1.15) both}
.mark path{stroke-dasharray:40;stroke-dashoffset:40;animation:draw .5s .25s ease-out forwards}
h1,.lead,.hint{animation:fade .5s .15s ease-out both}}
@keyframes rise{from{opacity:0;transform:scale(.7)}to{opacity:1;transform:none}}
@keyframes draw{to{stroke-dashoffset:0}}
@keyframes fade{from{opacity:0;transform:translateY(6px)}to{opacity:1;transform:none}}
"#;

/// Renders the result page in `lang` (the interface's language). `reason` (already user-facing
/// text in that language) replaces the default failure explanation. Nothing technical (codes,
/// ids) ever goes on this page.
pub(crate) fn result_page(success: bool, reason: Option<&str>, lang: Lang) -> String {
    let (class, title, lead, icon) = if success {
        (
            "ok",
            lang.pick("Hesabın onaylandı", "Your account is confirmed"),
            lang.pick(
                "Girişin Projects Launcher&#39;da birkaç saniye içinde tamamlanacak.",
                "Projects Launcher will finish signing you in within a few seconds.",
            )
            .to_owned(),
            r#"<path d="M9 18.5l6 6L27 12"/>"#,
        )
    } else {
        let retry = lang.pick(
            "Projects Launcher&#39;da tekrar dene.",
            "Try again in Projects Launcher.",
        );
        let lead = match reason {
            Some(reason) => format!("{} {retry}", escape_html(reason)),
            None => retry.to_owned(),
        };
        (
            "fail",
            lang.pick("Giriş tamamlanamadı", "Sign-in didn't finish"),
            lead,
            r#"<path d="M12 12l12 12M24 12L12 24"/>"#,
        )
    };
    let hint = lang.pick(
        "Bu sekmeyi kapatıp <b>Projects Launcher</b>&#39;a dönebilirsin.",
        "You can close this tab and go back to <b>Projects Launcher</b>.",
    );
    format!(
        concat!(
            "<!doctype html><html lang=\"{lang}\"><head><meta charset=\"utf-8\">",
            "<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">",
            "<meta name=\"color-scheme\" content=\"dark\">",
            "<meta name=\"referrer\" content=\"no-referrer\">",
            "<title>{title} · Projects Launcher</title><style>{style}</style></head>",
            "<body><main class=\"card {class}\" role=\"main\">",
            "<div class=\"brand\"><i aria-hidden=\"true\"></i>Projects Launcher</div>",
            "<div class=\"mark\" aria-hidden=\"true\"><svg viewBox=\"0 0 36 36\">{icon}</svg></div>",
            "<h1>{title}</h1><p class=\"lead\">{lead}</p>",
            "<p class=\"hint\">{hint}</p>",
            "</main></body></html>"
        ),
        lang = lang.code(),
        title = title,
        style = STYLE.trim(),
        class = class,
        icon = icon,
        lead = lead,
        hint = hint,
    )
}

fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_and_failure_variants() {
        let ok = result_page(true, None, Lang::Tr);
        assert!(ok.starts_with("<!doctype html>"));
        assert!(ok.contains("lang=\"tr\""));
        assert!(ok.contains("#0b0907"));
        assert!(ok.contains("#E8B040"));
        assert!(ok.contains("Hesabın onaylandı"));
        assert!(ok.contains("card ok"));
        assert!(ok.contains("birkaç saniye içinde tamamlanacak"));
        assert!(ok.contains("Bu sekmeyi kapatıp"));
        assert!(ok.contains("dönebilirsin."));
        assert!(!ok.contains("<script"));

        let fail = result_page(false, Some("Giriş izni <verilmedi>."), Lang::Tr);
        assert!(fail.contains("Giriş tamamlanamadı"));
        assert!(fail.contains("card fail"));
        assert!(
            fail.contains("Giriş izni &lt;verilmedi&gt;. Projects Launcher&#39;da tekrar dene.")
        );
        assert!(
            result_page(false, None, Lang::Tr).contains("Projects Launcher&#39;da tekrar dene.")
        );
    }

    #[test]
    fn the_page_speaks_the_interface_language() {
        let ok = result_page(true, None, Lang::En);
        assert!(ok.contains("lang=\"en\""));
        assert!(ok.contains("Your account is confirmed"));
        assert!(ok.contains("close this tab"));
        let fail = result_page(false, Some("Permission wasn't given."), Lang::En);
        assert!(
            fail.contains("Sign-in didn&#39;t finish") || fail.contains("Sign-in didn't finish")
        );
        assert!(fail.contains("Permission wasn&#39;t given. Try again in Projects Launcher."));
    }
}
