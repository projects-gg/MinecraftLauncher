package gg.projects.overlay.lang;

import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.Supplier;

/**
 * Which language the overlay shows (OVL-LANG): the launcher's word once it said one (the language the game started
 * in, {@code -Dprojects.overlay.lang}, then every {@code welcome}'s {@code lang}), else the game's own language (an
 * older launcher: the overlay follows the game as before, and its settings show no "Dil" row).
 *
 * <p>The player's pick in the settings goes to the launcher, which keeps it for every game and answers with a new
 * {@code welcome}: the panel changes when the launcher says so, never before, so its own words and the ones the
 * launcher writes for it (a sticker in words, a crew call) are always one language.
 */
public final class LanguageChoice {
    /** The JVM property the launcher starts the game with ({@code tr}, {@code en}). */
    public static final String PROPERTY = "projects.overlay.lang";
    /** Picks closer together than this are not sent (a held key, a double click: one change, never a flood). */
    public static final long PICK_GAP_MS = 500;

    private final Supplier<String> game;
    private final AtomicInteger revision = new AtomicInteger();
    /** The launcher's word, or null when it never said one. */
    private volatile Language said;
    /** This connection's welcome named a language: the launcher keeps a pick now. */
    private volatile boolean heard;
    private long pickedAt;
    private boolean picked;

    /** {@code started}: the launcher's {@link #PROPERTY} (null or anything else: none); {@code game}: the game's code. */
    public LanguageChoice(String started, Supplier<String> game) {
        this.game = game;
        this.said = Language.ofCode(started);
    }

    /** The language the overlay shows now. */
    public Language current() {
        Language s = said;
        return s != null ? s : Language.ofGame(game.get());
    }

    /** The launcher decides the language (the settings show the "Dil" row). */
    public boolean managed() {
        return said != null;
    }

    /** A pick can reach the launcher now (it named a language in this connection's welcome). */
    public boolean editable() {
        return heard;
    }

    /** Goes up whenever the launcher's word changes the language: what was laid out in the old one is laid out again. */
    public int revision() {
        return revision.get();
    }

    /**
     * A {@code welcome} came with {@code lang} (null or anything else: a launcher that does not say, which keeps no
     * pick). Returns whether the language the overlay shows changed.
     */
    public boolean welcome(String lang) {
        Language next = Language.ofCode(lang);
        heard = next != null;
        if (next == null) {
            return false;
        }
        Language before = current();
        said = next;
        if (next != before) {
            revision.incrementAndGet();
            return true;
        }
        return false;
    }

    /** The connection went: nothing keeps a pick until the next welcome. */
    public void offline() {
        heard = false;
    }

    /** The game language code the hello names: the overlay's when the launcher decides, else the game's own. */
    public String helloCode() {
        Language s = said;
        return s != null ? s.gameCode : game.get();
    }

    /**
     * The player picked {@code wanted} in the "Dil" row at {@code nowMs} (a monotonic clock): the code to send to the
     * launcher, or null when nothing goes (no launcher keeps it now, the overlay speaks it already, or the last pick was
     * less than {@link #PICK_GAP_MS} ago).
     */
    public synchronized String pick(Language wanted, long nowMs) {
        if (wanted == null || !heard || wanted == current()) {
            return null;
        }
        if (picked && nowMs - pickedAt < PICK_GAP_MS) {
            return null;
        }
        picked = true;
        pickedAt = nowMs;
        return wanted.code;
    }
}
