//! A signed Discord bot news feed for local end-to-end tests, and a checker for feeds the bot
//! writes. Never publish the fixture.
//!
//! `cargo run -p pl-content --example feed_fixture -- <out-dir> [1|2|status|tampered] [switches]`
//!
//! Writes `<out-dir>/v1/feed.json`, `.sig` and `media/` signed with a throwaway feed key stored
//! in `<out-dir>/feed-signing.key` (created on the first run, reused afterwards). Times are
//! relative to now: generation 1 has a live event (ends in two hours) linked to its news item,
//! an open-ended event tomorrow, patch notes without a cover and three covered items.
//! Generation 2 drops one item and the live event; `status` is generation 1 with an outage
//! notice that expires in 15 minutes; `tampered` writes generation 2 and then changes
//! `feed.json` after signing, so clients must refuse it.
//!
//! Switches, after the generation: `--level outage|major|degraded|notice` gives the status notice
//! that kind, with the bot's default wording (`status` only); `--expires-in <minutes>` makes it
//! last 1–15 minutes (`status` only); `--event-in <minutes>` adds an hour-long event that starts
//! that many minutes from now (the event card's "Hatırlat" and its reminder).
//!
//! Serve `<out-dir>` (`node D:/ProLauncher-build/shots/static-server.mjs <out-dir> 8789`) and
//! give a debug launcher `PROJECTS_FEED_MIRRORS=http://127.0.0.1:8789/v1/`; the content
//! fixture's config must enable `newsFeed` with the printed public key
//! (`e2e_fixture -- <dir> 1 --feed-key <public key>`).
//!
//! `cargo run -p pl-content --example feed_fixture -- check <feed-dir> <public-key>` verifies a
//! feed written by the bot (`<feed-dir>/feed.json`, `.sig`, `media/`) with the launcher's own
//! rules and prints what players would see.

use std::path::{Path, PathBuf};

use jiff::{SignedDuration, Timestamp};
use pl_content::{
    BotFeed, FeedIssue, MediaKind, SignatureFile, merge_events, merge_news, newest_patch_notes,
};
use serde_json::{Value, json};

fn write(base: &Path, relative: &str, bytes: &[u8]) {
    let path = base.join(relative);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("create folder");
    std::fs::write(&path, bytes).expect("write file");
}

fn art(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/launcher/src/assets/art")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Stores `bytes` under `media/` and returns its cover reference.
fn cover(v1: &Path, bytes: &[u8], width: u32, height: u32) -> Value {
    let media = pl_content::studio::media_ref(bytes, "webp", MediaKind::Image);
    write(v1, &media.path, bytes);
    let mut value = serde_json::to_value(&media).expect("media");
    value["width"] = json!(width);
    value["height"] = json!(height);
    value
}

fn iso(t: Timestamp) -> String {
    t.round(jiff::Unit::Second).expect("round").to_string()
}

fn source(channel: &str, channel_id: &str, message: &str) -> Value {
    json!({
        "kind": "discord",
        "channel": channel,
        "url": format!("https://discord.com/channels/480094926164590593/{channel_id}/{message}")
    })
}

const ANNOUNCEMENTS: &str = "480099230275600394";
const CHANGES: &str = "480099524652695576";

fn generation(v1: &Path, release: &str, now: Timestamp) -> Value {
    let hours = |h: i64| now + SignedDuration::from_hours(h);
    let winter = cover(v1, &art("news-winter.webp"), 1280, 720);
    let arena = cover(v1, &art("news-pvp-arena.webp"), 1280, 720);
    let build = cover(v1, &art("news-build-contest.webp"), 1280, 720);

    let mut items = vec![
        json!({
            "id": "discord-1300000000000000001",
            "title": "Kış etkinliği başladı",
            "summary": "Karlı köy açıldı: buz pateni yarışları ve sınırlı süreli kozmetikler seni bekliyor.",
            "body": "Kış etkinliği **şu an** sunucuda.\n\n## Neler var\n\n- Karlı köy ve buz pateni pisti\n- Günlük hediye kutuları\n- Sınırlı süreli pelerinler\n\nAyrıntılar için [etkinlik sayfasına](https://projects.gg/kis) bak.",
            "cover": winter,
            "category": "event",
            "tags": ["etkinlik"],
            "publishedAt": iso(hours(-1)),
            "author": "Projects Ekibi",
            "source": source("duyurular", ANNOUNCEMENTS, "1300000000000000001")
        }),
        json!({
            "id": "discord-1300000000000000002",
            "title": "2.4 yama notları",
            "summary": "Yeni zindan, PvP dengelemeleri ve hata düzeltmeleri.",
            "body": "- Yeni zindan: Kül Mağarası\n- Kılıç hasarı dengelendi\n- Arenada takılma düzeltildi\n- Market menüsü hızlandı",
            "category": "patchNotes",
            "tags": [],
            "publishedAt": iso(hours(-3)),
            "author": "Projects Ekibi",
            "source": source("değişiklikler", CHANGES, "1300000000000000002")
        }),
        json!({
            "id": "discord-1300000000000000003",
            "title": "Cumartesi PvP turnuvası kayıtları açıldı",
            "summary": "Takımını kur, cuma gece yarısına kadar kaydol.",
            "body": "Takımlar en fazla üç kişi olabilir. İlk üçe özel pelerin ve kredi ödülü var.",
            "cover": arena,
            "category": "announcement",
            "tags": [],
            "publishedAt": iso(hours(-26)),
            "author": "Projects Ekibi",
            "source": source("duyurular", ANNOUNCEMENTS, "1300000000000000003")
        }),
        json!({
            "id": "discord-1300000000000000004",
            "title": "İnşa yarışmasının kazananları",
            "summary": "Gökyüzü şehri temalı yarışmada ilk üç belli oldu.",
            "body": "Katılan herkese teşekkürler! Kazanan yapılar spawn'da sergileniyor.",
            "cover": build,
            "category": "community",
            "tags": [],
            "publishedAt": iso(hours(-50)),
            "author": "Projects Ekibi",
            "source": source("duyurular", ANNOUNCEMENTS, "1300000000000000004")
        }),
    ];
    let mut events = vec![
        json!({
            "id": "discord-1300000000000000001",
            "title": "Kış etkinliği",
            "startsAt": iso(hours(-1)),
            "endsAt": iso(hours(2)),
            "newsId": "discord-1300000000000000001",
            "source": source("duyurular", ANNOUNCEMENTS, "1300000000000000001")
        }),
        json!({
            "id": "discord-1300000000000000003",
            "title": "PvP turnuvası",
            "startsAt": iso(hours(26)),
            "newsId": "discord-1300000000000000003",
            "source": source("duyurular", ANNOUNCEMENTS, "1300000000000000003")
        }),
    ];
    if release != "1" {
        // The tournament post was deleted on Discord; the winter event ended early.
        items.retain(|item| item["id"] != "discord-1300000000000000003");
        events.retain(|event| event["id"] != "discord-1300000000000000001");
        for event in &mut events {
            event.as_object_mut().expect("event").remove("newsId");
        }
    }
    json!({
        "schema": 1,
        "generatedAt": iso(now),
        "items": items,
        "events": events
    })
}

fn signing_key(out: &Path) -> pl_content::SigningKey {
    let key_path = out.join("feed-signing.key");
    match std::fs::read_to_string(&key_path) {
        Ok(text) => pl_content::decode_signing_key(text.trim()).expect("feed test key"),
        Err(_) => {
            let key = pl_content::generate_signing_key().expect("OS random source");
            std::fs::create_dir_all(out).expect("create output");
            std::fs::write(&key_path, pl_content::encode_signing_key(&key))
                .expect("save feed test key");
            key
        }
    }
}

/// The switches after the generation (see the module docs).
struct Extras {
    /// The status notice's kind: `outage`, `major`, `degraded` or `notice`.
    level: String,
    /// Minutes until the status notice expires.
    expires_in: i64,
    /// Minutes until the extra event starts.
    event_in: Option<i64>,
}

impl Default for Extras {
    fn default() -> Self {
        Self {
            level: "outage".to_owned(),
            expires_in: 15,
            event_in: None,
        }
    }
}

impl Extras {
    /// `None` for an unknown switch or a bad value.
    fn parse(args: &[String]) -> Option<Self> {
        let mut extras = Self::default();
        let mut args = args.iter();
        while let Some(switch) = args.next() {
            let value = args.next()?;
            match switch.as_str() {
                "--level"
                    if ["outage", "major", "degraded", "notice"].contains(&value.as_str()) =>
                {
                    extras.level.clone_from(value);
                }
                "--expires-in" => {
                    extras.expires_in = value.parse().ok().filter(|m| (1..=15).contains(m))?;
                }
                "--event-in" => extras.event_in = Some(value.parse().ok()?),
                _ => return None,
            }
        }
        Some(extras)
    }
}

/// The status notice the bot writes for `level` (its default wording), updated now and expiring
/// `minutes` later.
fn status_block(level: &str, now: Timestamp, minutes: i64) -> Value {
    let (kind, title, message, services): (&str, &str, &str, &[(&str, &str)]) = match level {
        "major" => (
            "outage",
            "Hizmetlerde büyük bir kesinti var",
            "Şu an erişilemiyor: Lobi, Survival Zephyr, Skyblock ve 2 hizmet daha",
            &[
                ("Lobi", "down"),
                ("Survival Zephyr", "down"),
                ("Skyblock", "down"),
                ("Bedwars", "down"),
                ("Giriş", "down"),
            ],
        ),
        "degraded" => (
            "degraded",
            "Bazı hizmetlerde aksaklık var",
            "Bağlanırken sorun yaşanabilir: Survival Zephyr",
            &[("Survival Zephyr", "degraded")],
        ),
        "notice" => ("notice", "Durum sayfasında yeni bir duyuru var", "", &[]),
        _ => (
            "outage",
            "Bazı hizmetlerde sorun var",
            "Şu an erişilemiyor: Survival Zephyr",
            &[("Survival Zephyr", "down")],
        ),
    };
    let services: Vec<Value> = services
        .iter()
        .map(|(name, state)| json!({ "name": name, "state": state }))
        .collect();
    json!({
        "level": kind,
        "title": title,
        "message": message,
        "services": services,
        "url": "https://status.projects.gg",
        "updatedAt": iso(now),
        "expiresAt": iso(now + SignedDuration::from_mins(minutes))
    })
}

fn build(out: PathBuf, release: &str, extras: &Extras) {
    let v1 = out.join("v1");
    std::fs::create_dir_all(&v1).expect("create output");
    let signing = signing_key(&out);
    let now = Timestamp::now();

    let previous = std::fs::read(v1.join("feed.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|feed| feed["sequence"].as_u64())
        .unwrap_or(0);
    let unix = u64::try_from(now.as_second()).unwrap_or(0);
    let generation_of = if release == "status" { "1" } else { release };
    let mut feed = generation(&v1, generation_of, now);
    feed["sequence"] = json!(unix.max(previous + 1));
    if release == "status" {
        feed["status"] = status_block(&extras.level, now, extras.expires_in);
    }
    if let Some(minutes) = extras.event_in {
        let starts = now + SignedDuration::from_mins(minutes);
        let mut event = json!({
            "id": "discord-1300000000000000005",
            "title": "Akşam PvP turnuvası",
            "startsAt": iso(starts),
            "endsAt": iso(starts + SignedDuration::from_hours(1)),
            "source": source("duyurular", ANNOUNCEMENTS, "1300000000000000005")
        });
        // "Göster" opens the tournament's news item where this generation has it.
        if generation_of == "1" {
            event["newsId"] = json!("discord-1300000000000000003");
        }
        feed["events"].as_array_mut().expect("events").push(event);
    }
    let bytes = pl_content::json::to_canonical_json(&feed).expect("feed json");
    let signature = pl_content::sign_index(&bytes, &signing);
    let signature = pl_content::json::to_canonical_json(&signature).expect("signature json");

    write(&v1, "feed.json.sig", &signature);
    if release == "tampered" {
        let text = String::from_utf8(bytes)
            .expect("utf-8")
            .replace("2.4 yama notları", "2.4 yama notları (sahte)");
        write(&v1, "feed.json", text.as_bytes());
    } else {
        write(&v1, "feed.json", &bytes);
    }
    println!("feed written to {}", v1.display());
    println!("generation: {release}, sequence {}", feed["sequence"]);
    println!(
        "public key: {}",
        pl_content::encode_verifying_key(&signing.verifying_key())
    );
    println!(
        "key id:     {}",
        pl_content::key_id(&signing.verifying_key())
    );
}

/// Verifies a feed directory the way the launcher does and prints what players would see.
fn check(dir: PathBuf, public_key: &str) {
    let key = pl_content::decode_verifying_key(public_key).expect("public key");
    let bytes = std::fs::read(dir.join("feed.json")).expect("feed.json");
    let signature: SignatureFile =
        serde_json::from_slice(&std::fs::read(dir.join("feed.json.sig")).expect("feed.json.sig"))
            .expect("signature JSON");
    match pl_content::verify_index(&bytes, &signature, &[key]) {
        Ok(_) => println!("signature:  OK (key id {})", signature.key_id),
        Err(error) => {
            println!("signature:  REFUSED ({error})");
            std::process::exit(1);
        }
    }
    let (feed, problems) = match BotFeed::parse(&bytes) {
        Ok(parsed) => parsed,
        Err(error) => {
            println!("feed:       REFUSED ({error})");
            std::process::exit(1);
        }
    };
    println!(
        "feed:       schema {}, sequence {}, generated {}, {} bytes",
        feed.schema,
        feed.sequence,
        feed.generated_at,
        bytes.len()
    );
    for problem in &problems {
        let effect = if problem.issue.drops() {
            "dropped"
        } else {
            "repaired"
        };
        println!(
            "problem:    {:?} {} {:?} ({effect})",
            problem.target, problem.id, problem.issue
        );
    }
    let mut media_ok = true;
    for media in feed.media_refs() {
        let path = dir.join(&media.path);
        let verdict = match std::fs::read(&path) {
            Ok(file) => {
                let sha = pl_net::hash_bytes(&file, pl_net::HashAlgo::Sha256);
                if file.len() as u64 == media.size && sha == media.sha256 {
                    "OK"
                } else {
                    "MISMATCH"
                }
            }
            Err(_) => "MISSING",
        };
        media_ok &= verdict == "OK";
        println!("media:      {} {} bytes {verdict}", media.path, media.size);
    }
    let now = Timestamp::now();
    let news = merge_news(&pl_content::NewsFeed::default(), Some(&feed), 60, now);
    for entry in &news {
        println!(
            "news:       {} [{:?}] {} ({})",
            entry.item.id, entry.item.category, entry.item.title, entry.item.published_at
        );
    }
    if let Some(notes) = newest_patch_notes(&news) {
        println!("patchNotes: {}", notes.item.title);
    }
    for (i, event) in merge_events(&[], Some(&feed), &news, now)
        .iter()
        .enumerate()
    {
        println!(
            "event{}:     {} {:?} {} → {}{}",
            if i == 0 { "*" } else { " " },
            event.title,
            event.phase(now),
            event.starts_at,
            event.ends_at,
            if event.open_ended {
                " (open-ended)"
            } else {
                ""
            }
        );
    }
    match &feed.status {
        Some(status) => println!(
            "status:     {:?} {:?} / {:?}, {} services, until {} ({}){}",
            status.level,
            status.title,
            status.message,
            status.services.len(),
            status.expires_at,
            if status.is_active(now) {
                "shown"
            } else {
                "expired"
            },
            status
                .url
                .as_deref()
                .map(|url| format!(", link {url}"))
                .unwrap_or_default()
        ),
        None => println!("status:     none"),
    }
    let dropped = problems.iter().filter(|p| p.issue.drops()).count();
    let unsafe_links = problems
        .iter()
        .filter(|p| p.issue == FeedIssue::UnsafeBodyLink)
        .count();
    println!(
        "summary:    {} items, {} events, {dropped} dropped, {unsafe_links} with plain-text links, media {}",
        feed.items.len(),
        feed.events.len(),
        if media_ok { "OK" } else { "NOT OK" }
    );
}

fn usage() -> ! {
    eprintln!(
        "usage: feed_fixture <out-dir> [1|2|status|tampered] [--level outage|major|degraded|notice] \
         [--expires-in <minutes>] [--event-in <minutes>]\n       feed_fixture check <feed-dir> <public-key>"
    );
    std::process::exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [mode, dir, key] if mode == "check" => check(PathBuf::from(dir), key),
        [out] => build(PathBuf::from(out), "1", &Extras::default()),
        [out, release, switches @ ..]
            if ["1", "2", "status", "tampered"].contains(&release.as_str()) =>
        {
            let extras = Extras::parse(switches).unwrap_or_else(|| usage());
            build(PathBuf::from(out), release, &extras);
        }
        _ => usage(),
    }
}
