package gg.projects.overlay.emoji;

import java.util.List;
import java.util.function.IntPredicate;

/**
 * The picker's own set for every server but Projects (and singleplayer): symbols and expressions written into the chat
 * line as they are, which any server shows with the game's font. Only what that font draws is offered: an entry with a
 * character the font lacks (a resource pack may change it) is left out, so the picker never shows an empty box.
 */
public final class UnicodeSet {
    private static final List<Emoji> ALL = List.of(
        Emoji.plain("❤", EmojiKind.SYMBOL, "Kalp", "Heart"),
        Emoji.plain("★", EmojiKind.SYMBOL, "Yıldız", "Star"),
        Emoji.plain("✔", EmojiKind.SYMBOL, "Tamam", "Check"),
        Emoji.plain("✖", EmojiKind.SYMBOL, "Çarpı", "Cross"),
        Emoji.plain("☺", EmojiKind.SYMBOL, "Gülen yüz", "Smiling face"),
        Emoji.plain("☹", EmojiKind.SYMBOL, "Üzgün yüz", "Frowning face"),
        Emoji.plain("☻", EmojiKind.SYMBOL, "Dolu gülen yüz", "Solid smiling face"),
        Emoji.plain("👍", EmojiKind.SYMBOL, "Beğendim", "Thumbs up"),
        Emoji.plain("👎", EmojiKind.SYMBOL, "Beğenmedim", "Thumbs down"),
        Emoji.plain("👋", EmojiKind.SYMBOL, "El sallama", "Waving hand"),
        Emoji.plain("✌", EmojiKind.SYMBOL, "Zafer", "Victory hand"),
        Emoji.plain("👀", EmojiKind.SYMBOL, "Gözler", "Eyes"),
        Emoji.plain("💀", EmojiKind.SYMBOL, "Kafatası", "Skull"),
        Emoji.plain("🔥", EmojiKind.SYMBOL, "Ateş", "Fire"),
        Emoji.plain("💎", EmojiKind.SYMBOL, "Elmas", "Gem"),
        Emoji.plain("⚔", EmojiKind.SYMBOL, "Kılıçlar", "Crossed swords"),
        Emoji.plain("🗡", EmojiKind.SYMBOL, "Hançer", "Dagger"),
        Emoji.plain("🏹", EmojiKind.SYMBOL, "Yay ve ok", "Bow and arrow"),
        Emoji.plain("⛏", EmojiKind.SYMBOL, "Kazma", "Pickaxe"),
        Emoji.plain("🛡", EmojiKind.SYMBOL, "Kalkan", "Shield"),
        Emoji.plain("☠", EmojiKind.SYMBOL, "Korsan", "Skull and crossbones"),
        Emoji.plain("⚡", EmojiKind.SYMBOL, "Şimşek", "Lightning"),
        Emoji.plain("☀", EmojiKind.SYMBOL, "Güneş", "Sun"),
        Emoji.plain("☁", EmojiKind.SYMBOL, "Bulut", "Cloud"),
        Emoji.plain("☂", EmojiKind.SYMBOL, "Şemsiye", "Umbrella"),
        Emoji.plain("❄", EmojiKind.SYMBOL, "Kar tanesi", "Snowflake"),
        Emoji.plain("☃", EmojiKind.SYMBOL, "Kardan adam", "Snowman"),
        Emoji.plain("♪", EmojiKind.SYMBOL, "Nota", "Note"),
        Emoji.plain("♫", EmojiKind.SYMBOL, "Notalar", "Notes"),
        Emoji.plain("♛", EmojiKind.SYMBOL, "Taç", "Crown"),
        Emoji.plain("⚑", EmojiKind.SYMBOL, "Bayrak", "Flag"),
        Emoji.plain("☕", EmojiKind.SYMBOL, "Kahve", "Coffee"),
        Emoji.plain("⌛", EmojiKind.SYMBOL, "Kum saati", "Hourglass"),
        Emoji.plain("✦", EmojiKind.SYMBOL, "Parıltı", "Sparkle"),
        Emoji.plain("∞", EmojiKind.SYMBOL, "Sonsuz", "Infinity"),
        Emoji.plain("➜", EmojiKind.SYMBOL, "Ok", "Arrow"),
        Emoji.plain("¯\\_(ツ)_/¯", EmojiKind.TEXT, "Omuz silkme", "Shrug"),
        Emoji.plain("(╯°□°)╯︵ ┻━┻", EmojiKind.TEXT, "Masa fırlatma", "Table flip"),
        Emoji.plain("┬─┬ノ( º _ ºノ)", EmojiKind.TEXT, "Masayı düzeltme", "Table back"),
        Emoji.plain("( •_•)>⌐■-■", EmojiKind.TEXT, "Gözlük takma", "Putting on shades"),
        Emoji.plain("(⌐■_■)", EmojiKind.TEXT, "Havalı", "Cool"),
        Emoji.plain("ʕ•ᴥ•ʔ", EmojiKind.TEXT, "Ayıcık", "Bear"),
        Emoji.plain("(づ｡◕‿‿◕｡)づ", EmojiKind.TEXT, "Sarılma", "Hug"),
        Emoji.plain("ಠ_ಠ", EmojiKind.TEXT, "Ters bakış", "Disapproval"),
        Emoji.plain("(ﾉ◕ヮ◕)ﾉ*:･ﾟ✧", EmojiKind.TEXT, "Coşku", "Excitement"),
        Emoji.plain("(☞ﾟヮﾟ)☞", EmojiKind.TEXT, "Sen", "You"),
        Emoji.plain("(╥﹏╥)", EmojiKind.TEXT, "Ağlama", "Crying"),
        Emoji.plain("(•_•)", EmojiKind.TEXT, "Boş bakış", "Blank stare"),
        Emoji.plain("(o_O)", EmojiKind.TEXT, "Şaşkın", "Confused"),
        Emoji.plain("\\(^o^)/", EmojiKind.TEXT, "Yaşasın", "Hooray"),
        Emoji.plain("(｡◕‿◕｡)", EmojiKind.TEXT, "Tatlı", "Sweet"),
        Emoji.plain("┐(´ー｀)┌", EmojiKind.TEXT, "Bilmem", "Whatever"),
        Emoji.plain("✌(◕‿-)✌", EmojiKind.TEXT, "Göz kırpma", "Wink"),
        Emoji.plain("ヘ( ^o^)ノ＼(^_^ )", EmojiKind.TEXT, "Çak bir beşlik", "High five"),
        Emoji.plain("GG", EmojiKind.TEXT, "İyi oyundu", "Good game"));

    private UnicodeSet() {
    }

    /** Every entry, whatever the font draws. */
    public static List<Emoji> all() {
        return ALL;
    }

    /** The entries whose every character {@code draws} (the game's font has it). */
    public static List<Emoji> drawable(IntPredicate draws) {
        return ALL.stream().filter(e -> e.code().codePoints().allMatch(cp -> cp == ' ' || draws.test(cp))).toList();
    }
}
