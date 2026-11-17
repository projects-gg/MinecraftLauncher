//! The page the browser shows after Microsoft redirects back to the launcher.
//!
//! It wears the same template as the friends service's browser pages (social.projects.gg): the
//! season's key art, lightly blurred, under the PROJECTS.GG wordmark, one obsidian panel with the
//! state's small mark, Tektur titles and Figtree text (docs/design.md §1 "The art lights the
//! app"). The loopback answers a single request, so the pictures and fonts travel inside the page
//! as `data:` URIs (sources and licences: `assets/page/README.md`).

use std::sync::LazyLock;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use pl_lang::Lang;

/// Content-Security-Policy sent with the page: inline styles and embedded pictures and fonts only;
/// no scripts, no requests.
pub(crate) const PAGE_CSP: &str = "default-src 'none'; img-src data:; font-src data:; \
                                   style-src 'unsafe-inline'; base-uri 'none'; \
                                   form-action 'none'; frame-ancestors 'none'";

const ART: &[u8] = include_bytes!("../assets/page/art-1280.webp");
const ART_SIZE: (u32, u32) = (1280, 720);
const WORDMARK: &[u8] = include_bytes!("../assets/page/wordmark-320.webp");
/// Shown 64 px high (52 px on phones); the source is 1393 × 489.
const WORDMARK_SIZE: (u32, u32) = (182, 64);
const ICON: &[u8] = include_bytes!("../assets/page/icon-32.png");
const TEKTUR: &[u8] = include_bytes!("../assets/page/tektur-latin.woff2");
const TEKTUR_TR: &[u8] = include_bytes!("../assets/page/tektur-tr.woff2");
const FIGTREE: &[u8] = include_bytes!("../assets/page/figtree-latin.woff2");
const FIGTREE_TR: &[u8] = include_bytes!("../assets/page/figtree-tr.woff2");

/// The latin subsets (Latin-1 and punctuation, "ı" beside "r") and the Turkish letters of latin-ext.
const LATIN: &str = "U+0000-00FF,U+0131,U+0152-0153,U+02BB-02BC,U+02C6,U+02DA,U+02DC,U+0304,\
                     U+0308,U+0329,U+2000-206F,U+20AC,U+2122,U+2191,U+2193,U+2212,U+2215,U+FEFF,\
                     U+FFFD";
const TURKISH: &str = "U+011E-011F,U+0130,U+015E-015F";

fn data_uri(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", STANDARD.encode(bytes))
}

/// Everything that does not depend on the outcome or the language, encoded once.
struct Shell {
    style: String,
    art: String,
    wordmark: String,
    icon: String,
}

static SHELL: LazyLock<Shell> = LazyLock::new(|| {
    let face = |family: &str, weights: &str, bytes: &[u8], range: &str| {
        format!(
            "@font-face{{font-family:{family};font-weight:{weights};font-display:swap;\
             src:url({}) format(\"woff2\");unicode-range:{range}}}",
            data_uri("font/woff2", bytes)
        )
    };
    let style = [
        face("Tektur", "400 900", TEKTUR, LATIN),
        face("Tektur", "400 900", TEKTUR_TR, TURKISH),
        face("Figtree", "300 900", FIGTREE, LATIN),
        face("Figtree", "300 900", FIGTREE_TR, TURKISH),
        STYLE.trim().to_owned(),
    ]
    .join("\n");
    Shell {
        style,
        art: data_uri("image/webp", ART),
        wordmark: data_uri("image/webp", WORDMARK),
        icon: data_uri("image/png", ICON),
    }
});

/// The friends service's page template (`services/social/src/pages.ts`), less the parts this page
/// has no use for (actions, codes, crews).
const STYLE: &str = r#"
:root{color-scheme:dark;--bone:#f3ebdd;--bone-50:#fbf6ec;--ash-300:#bdb6c3;--ash-400:#9b93a3;
--ease:cubic-bezier(.16,1,.3,1);--settle:cubic-bezier(.33,1,.68,1);--gutter:clamp(16px,5.2vw,80px);
--title:clamp(24px,1.25vw + 16px,34px);
--sans:Figtree,ui-sans-serif,system-ui,-apple-system,"Segoe UI",Roboto,sans-serif;
--display:Tektur,ui-sans-serif,system-ui,-apple-system,"Segoe UI",Roboto,sans-serif}
*{box-sizing:border-box}
html{background:#0a090c;-webkit-text-size-adjust:100%;text-size-adjust:100%}
body{margin:0;min-height:100vh;min-height:100dvh;display:grid;grid-template-rows:auto 1fr;
padding:clamp(20px,4.6vh,40px) var(--gutter) clamp(20px,6vh,56px);color:var(--bone);
font:450 16px/1.55 var(--sans);overflow-wrap:break-word;-webkit-font-smoothing:antialiased;
-moz-osx-font-smoothing:grayscale}
::selection{background:rgb(232 154 44/.45);color:var(--bone-50)}
.art{position:fixed;inset:0;z-index:-1;overflow:hidden;pointer-events:none;
background:radial-gradient(70% 80% at 60% 30%,#5a1d10,#2a0909 45%,#0a090c)}
.art img{position:absolute;inset:0;width:100%;height:100%;object-fit:cover;object-position:62% 30%;
transform-origin:62% 30%;animation:art .6s var(--settle) 80ms both}
.art::after{content:"";position:absolute;inset:0;z-index:1;background:#0a090c;opacity:.72;
animation:veil .56s var(--settle) 80ms both}
.top{display:flex;align-items:center;animation:rise .46s var(--ease) .12s both}
.wordmark{display:block;width:auto;height:64px}
.panel{align-self:center;justify-self:start;width:100%;max-width:clamp(440px,36vw,520px);margin:32px 0 0;
padding:32px 32px 28px;border-radius:10px;background:rgb(18 16 22/.8);border:1px solid rgb(255 255 255/.07);
box-shadow:inset 0 1px 0 rgb(255 255 255/.05),0 32px 80px -32px rgb(63 10 10/.85),0 18px 40px -24px rgb(10 9 12/.9);
animation:rise .46s var(--ease) .16s both}
.panel>:last-child{margin-bottom:0}
.head{display:flex;align-items:flex-start;gap:12px;margin:0 0 10px}
.status{flex:none;display:block;width:30px;height:30px;margin-top:max(0px,calc((var(--title) * 1.12 - 30px) / 2))}
.status svg{display:block;width:100%;height:100%}
.ok{color:#5fcb8e}.bad{color:#f0624d}
h1{margin:0;font:600 var(--title)/1.12 var(--display);letter-spacing:-.006em;color:var(--bone-50);
text-wrap:balance}
p{margin:0 0 12px;color:var(--ash-300);text-wrap:pretty}
strong{color:var(--bone-50);font-weight:650}
.note{margin:22px 0 0;padding-top:16px;border-top:1px solid rgb(255 255 255/.07);font-size:13.5px;
line-height:1.5;color:var(--ash-400)}
@media (max-width:759px){
.wordmark{height:52px}
.panel{align-self:end;max-width:none;margin-top:24px;padding:26px 22px 22px}}
@media (prefers-reduced-motion:reduce){
.art img{animation:none}
.art::after{animation:veil .3s ease-out 80ms both}
.top{animation:fade .3s ease-out .12s both}
.panel{animation:fade .3s ease-out .16s both}}
@media (prefers-reduced-transparency:reduce){.panel{background:#121016}}
@keyframes art{from{transform:scale(1.05)}}
@keyframes veil{to{opacity:0}}
@keyframes rise{from{opacity:0;transform:translateY(16px)}}
@keyframes fade{from{opacity:0}}
"#;

// Status marks: Phosphor icons ("duotone"; MIT License, the launcher's icon set).
const RING: &str = r#"<path d="M224,128a96,96,0,1,1-96-96A96,96,0,0,1,224,128Z" opacity="0.2"/>"#;
const CIRCLE: &str = "M232,128A104,104,0,1,1,128,24,104.11,104.11,0,0,1,232,128Zm-16,0a88,88,0,1,0-88,88A88.1,88.1,0,0,0,216,128Z";
const CHECK: &str = "M173.66,98.34a8,8,0,0,1,0,11.32l-56,56a8,8,0,0,1-11.32,0l-24-24a8,8,0,0,1,11.32-11.32L112,148.69l50.34-50.35A8,8,0,0,1,173.66,98.34Z";
const CROSS: &str = "M165.66,101.66,139.31,128l26.35,26.34a8,8,0,0,1-11.32,11.32L128,139.31l-26.34,26.35a8,8,0,0,1-11.32-11.32L116.69,128,90.34,101.66a8,8,0,0,1,11.32-11.32L128,116.69l26.34-26.35a8,8,0,0,1,11.32,11.32Z";

/// Renders the result page in `lang` (the interface's language). `reason` (already user-facing
/// text in that language) replaces the default failure explanation. Nothing technical (codes,
/// ids) ever goes on this page.
pub(crate) fn result_page(success: bool, reason: Option<&str>, lang: Lang) -> String {
    let (tone, title, lead, mark) = if success {
        (
            "ok",
            lang.pick("Hesabın Onaylandı", "Account Confirmed"),
            lang.pick(
                "Girişin Projects Launcher&#39;da birkaç saniye içinde tamamlanacak.",
                "Projects Launcher will finish signing you in within a few seconds.",
            )
            .to_owned(),
            CHECK,
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
            "bad",
            lang.pick("Giriş Tamamlanamadı", "Sign-In Didn&#39;t Finish"),
            lead,
            CROSS,
        )
    };
    let note = lang.pick("Bu sekmeyi kapatabilirsin.", "You can close this tab.");
    let shell = &*SHELL;
    format!(
        concat!(
            "<!doctype html>\n<html lang=\"{lang}\">\n<head>\n<meta charset=\"utf-8\">\n",
            "<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n",
            "<meta name=\"robots\" content=\"noindex,nofollow\">\n",
            "<meta name=\"referrer\" content=\"no-referrer\">\n",
            "<meta name=\"theme-color\" content=\"#0a090c\">\n",
            "<title>{title} · Projects Launcher</title>\n",
            "<link rel=\"icon\" type=\"image/png\" sizes=\"32x32\" href=\"{icon}\">\n",
            "<style>{style}</style>\n</head>\n<body>\n",
            "<div class=\"art\" aria-hidden=\"true\"><img src=\"{art}\" width=\"{art_w}\" ",
            "height=\"{art_h}\" alt=\"\" decoding=\"async\"></div>\n",
            "<header class=\"top\"><img class=\"wordmark\" src=\"{wordmark}\" width=\"{mark_w}\" ",
            "height=\"{mark_h}\" alt=\"PROJECTS.GG\"></header>\n",
            "<main class=\"panel\">\n",
            "<div class=\"head\"><span class=\"status {tone}\"><svg viewBox=\"0 0 256 256\" ",
            "fill=\"currentColor\" aria-hidden=\"true\">{ring}<path d=\"{mark}{circle}\"/></svg></span>\n",
            "<h1>{title}</h1></div>\n",
            "<p>{lead}</p>\n",
            "<p class=\"note\">{note}</p>\n",
            "</main>\n</body>\n</html>"
        ),
        lang = lang.code(),
        title = title,
        icon = shell.icon,
        style = shell.style,
        art = shell.art,
        art_w = ART_SIZE.0,
        art_h = ART_SIZE.1,
        wordmark = shell.wordmark,
        mark_w = WORDMARK_SIZE.0,
        mark_h = WORDMARK_SIZE.1,
        tone = tone,
        ring = RING,
        mark = mark,
        circle = CIRCLE,
        lead = lead,
        note = note,
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
        assert!(ok.contains("#0a090c"));
        assert!(ok.contains("Hesabın Onaylandı"));
        assert!(ok.contains("status ok"));
        assert!(ok.contains("birkaç saniye içinde tamamlanacak"));
        assert!(ok.contains("Bu sekmeyi kapatabilirsin."));
        assert!(!ok.contains("<script"));

        let fail = result_page(false, Some("Giriş izni <verilmedi>."), Lang::Tr);
        assert!(fail.contains("Giriş Tamamlanamadı"));
        assert!(fail.contains("status bad"));
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
        assert!(ok.contains("Account Confirmed"));
        assert!(ok.contains("close this tab"));
        let fail = result_page(false, Some("Permission wasn't given."), Lang::En);
        assert!(fail.contains("Sign-In Didn&#39;t Finish"));
        assert!(fail.contains("Permission wasn&#39;t given. Try again in Projects Launcher."));
    }

    #[test]
    fn pictures_and_fonts_are_inside_the_page_and_allowed_by_the_policy() {
        let page = result_page(true, None, Lang::Tr);
        // Nothing is fetched: every resource is a data: URI the policy allows.
        assert!(!page.contains("src=\"http"));
        assert!(!page.contains("url(http"));
        assert!(page.contains("src=\"data:image/webp;base64,"));
        assert_eq!(page.matches("url(data:font/woff2;base64,").count(), 4);
        assert!(page.contains("href=\"data:image/png;base64,"));
        assert!(PAGE_CSP.contains("default-src 'none'"));
        assert!(PAGE_CSP.contains("img-src data:"));
        assert!(PAGE_CSP.contains("font-src data:"));
        assert!(!PAGE_CSP.contains("script-src"));
    }
}
