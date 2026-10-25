//! Cloud AFK's notices in the game (AFKPLUS, `docs/tasks/afkplus.md` §8.5): the launcher's general
//! `info` card, in the game's language, for what a player wants to hear while playing (an approval
//! waiting, a deal done or failed, something found, an account that stopped, the AI's allowance,
//! new messages). The night's digest, daily and weekly summaries and closed approvals stay in the
//! launcher. No new frame: the mod already draws `info` cards (and its settings can turn them off).

use pl_social::AfkNotice;

use crate::protocol::{ToastKind, ToastView};

/// `37.500` (Turkish) or `37,500` (English): whole amounts of the game's money.
fn amount(text: &str, english: bool) -> Option<String> {
    let digits = text.trim();
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let digits = digits.trim_start_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let separator = if english { ',' } else { '.' };
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(separator);
        }
        out.push(c);
    }
    Some(out)
}

/// The item's name: the catalog's when the launcher knows it, else its id made readable
/// (`minecraft:diamond_sword` → "Diamond sword").
fn item(notice: &AfkNotice, english: bool) -> Option<String> {
    if let Some(name) = &notice.item_name {
        let text = if english { &name.en } else { &name.tr };
        if !text.trim().is_empty() {
            return Some(text.trim().to_owned());
        }
    }
    let id = notice.get("item")?;
    let path = id.rsplit(':').next().unwrap_or(id).replace('_', " ");
    let mut chars = path.trim().chars();
    let first = chars.next()?;
    Some(first.to_uppercase().chain(chars).collect())
}

/// "1 Elytra, 37.500" / "1 × Elytra, 37,500": what a deal or an approval is about. Turkish takes no
/// plural after a number; the game's English names are singular and their plurals cannot be
/// guessed ("Totem of Undying"), so English counts with "×" (as the launcher does).
fn deal(notice: &AfkNotice, english: bool) -> Option<String> {
    let item = item(notice, english)?;
    let qty = notice
        .get("qty")
        .filter(|qty| !qty.is_empty())
        .unwrap_or("1");
    let what = if english {
        format!("{qty} × {item}")
    } else {
        format!("{qty} {item}")
    };
    match notice.get("price").and_then(|price| amount(price, english)) {
        Some(price) => Some(format!("{what}, {price}")),
        None => Some(what),
    }
}

/// The in-game card of an AFK notice, or None for one that stays in the launcher.
pub(crate) fn afk_toast(notice: &AfkNotice, english: bool) -> Option<ToastView> {
    let pick = |tr: &str, en: &str| {
        if english {
            en.to_owned()
        } else {
            tr.to_owned()
        }
    };
    let body = match notice.event.as_str() {
        "approval" => {
            let what = deal(notice, english)?;
            pick(
                &format!("Onay bekliyor: {what}"),
                &format!("Waiting for approval: {what}"),
            )
        }
        "deal_done" => {
            let what = deal(notice, english)?;
            if notice.get("side") == Some("sell") {
                pick(&format!("Satıldı: {what}"), &format!("Sold: {what}"))
            } else {
                pick(&format!("Alındı: {what}"), &format!("Bought: {what}"))
            }
        }
        "deal_failed" => {
            let what = item(notice, english)?;
            pick(
                &format!("Takas olmadı: {what}"),
                &format!("Trade fell through: {what}"),
            )
        }
        "found" => {
            let what = item(notice, english)?;
            match notice.get("price").and_then(|price| amount(price, english)) {
                Some(price) => pick(
                    &format!("Bulundu: {what}, {price}"),
                    &format!("Found: {what}, {price}"),
                ),
                None => pick(&format!("Bulundu: {what}"), &format!("Found: {what}")),
            }
        }
        "stopped" => pick("Hesap durdu.", "The account stopped."),
        "expired" => pick("Hesabın süresi bitti.", "The account's time is up."),
        "renewal" => match notice.get("days").filter(|days| !days.is_empty()) {
            Some(days) => pick(
                &format!("Süre {days} gün sonra bitiyor."),
                &format!("Runs out in {days} days."),
            ),
            None => pick("Süre yakında bitiyor.", "Runs out soon."),
        },
        "quota" => match notice.get("which") {
            // The AI usage allowance is "kontör" / "AI credits" to the player (owner, 25 Oct).
            Some("full") => pick("Kontörün bitti.", "You're out of AI credits."),
            Some("80") => pick(
                "Kontörün çoğu kullanıldı.",
                "Most of your AI credits are used.",
            ),
            Some("gift") => pick(
                "Hediye kontörün yakında bitiyor.",
                "Your gift AI credits end soon.",
            ),
            _ => return None,
        },
        "incident" => pick(
            "Uygunsuz mesajlar geldi; o oyuncuya artık cevap verilmiyor.",
            "Inappropriate messages came; that player gets no replies now.",
        ),
        "message" => match notice
            .get("count")
            .and_then(|count| count.parse::<u32>().ok())
        {
            Some(count) if count > 1 => pick(
                &format!("{count} yeni mesaj."),
                &format!("{count} new messages."),
            ),
            _ => pick("Yeni mesaj.", "New message."),
        },
        "paused" => pick("Asistan durdu.", "The assistant paused."),
        // The night's digest, summaries and closed approvals are the launcher's.
        _ => return None,
    };
    // A plain 7/24 Bulut account's events carry its name (without one, the cloud's), never "AFK+"
    // (it may be off); the assistant's own say whose assistant it is, as its launcher panel does.
    let plain = matches!(notice.event.as_str(), "stopped" | "expired" | "renewal");
    let name = notice
        .get("name")
        .map(str::trim)
        .filter(|name| !name.is_empty());
    let title = match (name, plain) {
        (Some(name), true) => name.to_owned(),
        (Some(name), false) => pick(&format!("{name} asistanı"), &format!("{name}'s assistant")),
        (None, true) => pick("7/24 Bulut", "24/7 Cloud"),
        (None, false) => "AFK+".to_owned(),
    };
    let key = format!("afk:{}", notice.tag.as_deref().unwrap_or(&notice.id));
    Some(ToastView {
        kind: ToastKind::Info,
        key,
        title,
        body,
        conv: None,
        crew: None,
        sticker: None,
        badge: None,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pl_social::AfkItemName;

    use super::*;

    fn notice(event: &str, data: &[(&str, &str)]) -> AfkNotice {
        AfkNotice {
            id: "01JBXK4V7Q2M9R3T5W8Y0ZA1BC".into(),
            event: event.into(),
            kind: "afk_deal".into(),
            priority: "high".into(),
            at: 1_792_521_000,
            tag: None,
            quiet: false,
            data: data
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect::<BTreeMap<_, _>>(),
            item_name: None,
        }
    }

    #[test]
    fn amounts_are_grouped_in_the_games_language() {
        assert_eq!(amount("37500", false).as_deref(), Some("37.500"));
        assert_eq!(amount("37500", true).as_deref(), Some("37,500"));
        assert_eq!(amount("1250000", false).as_deref(), Some("1.250.000"));
        assert_eq!(amount("950", true).as_deref(), Some("950"));
        assert_eq!(amount("12a", true), None);
    }

    #[test]
    fn an_approval_becomes_an_info_card_of_the_bot() {
        let mut approval = notice(
            "approval",
            &[
                ("name", "Ahmet_AFK"),
                ("item", "minecraft:elytra"),
                ("qty", "1"),
                ("price", "37500"),
                ("side", "buy"),
            ],
        );
        approval.tag = Some("approval:01JBXK3D2E".into());
        let card = afk_toast(&approval, false).unwrap();
        assert_eq!(card.kind, ToastKind::Info);
        assert_eq!(card.title, "Ahmet_AFK asistanı");
        assert_eq!(card.body, "Onay bekliyor: 1 Elytra, 37.500");
        assert_eq!(card.key, "afk:approval:01JBXK3D2E");
        let card = afk_toast(&approval, true).unwrap();
        assert_eq!(card.title, "Ahmet_AFK's assistant");
        assert_eq!(card.body, "Waiting for approval: 1 × Elytra, 37,500");
        // A plain account's event keeps the bare name.
        let renewal = notice("renewal", &[("name", "Ahmet_AFK"), ("days", "3")]);
        assert_eq!(afk_toast(&renewal, false).unwrap().title, "Ahmet_AFK");
    }

    #[test]
    fn the_catalogs_name_wins_and_ids_read_plainly_without_it() {
        let mut done = notice(
            "deal_done",
            &[
                ("name", "Ahmet_AFK"),
                ("item", "minecraft:diamond"),
                ("qty", "64"),
                ("price", "7200"),
                ("side", "sell"),
            ],
        );
        assert_eq!(
            afk_toast(&done, false).unwrap().body,
            "Satıldı: 64 Diamond, 7.200"
        );
        done.item_name = Some(AfkItemName {
            tr: "Elmas".into(),
            en: "Diamond".into(),
        });
        assert_eq!(
            afk_toast(&done, false).unwrap().body,
            "Satıldı: 64 Elmas, 7.200"
        );
        assert_eq!(
            afk_toast(&done, true).unwrap().body,
            "Sold: 64 × Diamond, 7,200"
        );
        let found = notice(
            "found",
            &[("item", "minecraft:netherite_sword"), ("price", "90000")],
        );
        let card = afk_toast(&found, true).unwrap();
        assert_eq!(card.body, "Found: Netherite sword, 90,000");
        assert_eq!(card.title, "AFK+");
        // A plain account's event without a name is the cloud's, never "AFK+".
        let stopped = notice("stopped", &[]);
        assert_eq!(afk_toast(&stopped, false).unwrap().title, "7/24 Bulut");
        assert_eq!(afk_toast(&stopped, true).unwrap().title, "24/7 Cloud");
    }

    #[test]
    fn digests_and_closed_approvals_stay_in_the_launcher() {
        for event in ["night", "summary", "weekly", "approval_closed", "later"] {
            assert_eq!(
                afk_toast(&notice(event, &[("name", "A")]), false),
                None,
                "{event}"
            );
        }
        assert_eq!(
            afk_toast(&notice("quota", &[("which", "full")]), false)
                .unwrap()
                .body,
            "Kontörün bitti."
        );
        assert_eq!(
            afk_toast(&notice("message", &[("count", "3")]), true)
                .unwrap()
                .body,
            "3 new messages."
        );
    }
}
