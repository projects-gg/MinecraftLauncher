package gg.projects.overlay.link;

import com.google.gson.JsonObject;
import gg.projects.overlay.bridge.Protocol;

/**
 * Linking in game without a code (SKIN1 §3.7; docs/tasks/skin1-o.md), the mod's part:
 * <ol>
 * <li>a Projects server asks on {@code projects:link} whether the launcher vouches for the player;</li>
 * <li>the mod tells it {@code pending}, asks the launcher for its permit ({@code link-want}) and hands the permit
 * on ({@code intent}), or says it has none ({@code none});</li>
 * <li>after ProBot checked the permit the server sends its question; the mod shows it once no other screen is
 * open and sends the player's explicit "Evet, eşle" or "Hayır". Escape, closing it and leaving send nothing.</li>
 * </ol>
 * Nothing goes to a server that is not Projects, nothing goes unasked, and the permit is never kept: it passes
 * from the launcher's answer straight to the server that asked. Pure: the game side comes in as arguments and
 * leaves through {@link Out}. Every call on the game's thread.
 */
public final class CodelessLink {
    /** The server waits 15 s for the permit after its ask; the launcher gets this long of it. */
    static final long LAUNCHER_WAIT_MS = 10_000;
    /** A permit with less left than this is not handed on: ProBot still checks it and the player gets two minutes. */
    static final long PERMIT_MARGIN_MS = 3 * 60_000;
    /** A question that found no free screen in this long is let go (the server waits as long for its own window). */
    static final long SHOW_WAIT_MS = 30_000;
    /** The screen has to be free this long first: a question never jumps in as the chat closes. */
    static final long SETTLE_MS = 400;

    /** Where messages go: the server on {@code projects:link}, the launcher on the bridge. */
    public interface Out {
        void toServer(String json);

        void toLauncher(JsonObject frame);
    }

    private final Out out;
    /** An ask the launcher has not answered yet: since when (monotonic ms), and for which account. */
    private boolean asking;
    private long askedAt;
    private String askedName;
    /** The server's question, waiting for a free screen; and the one on screen. */
    private LinkMessages.Question waiting;
    private long waitingSince;
    /** The screen is free, since {@link #freeSince}. */
    private boolean free;
    private long freeSince;
    private LinkMessages.Question shown;

    public CodelessLink(Out out) {
        this.out = out;
    }

    /**
     * The server asked. Answered only on Projects: there {@code none} at once when the launcher cannot vouch,
     * else {@code pending} and the launcher is asked.
     *
     * @param projects the game is on a Projects server
     * @param name     the account the game plays as
     * @param vouches  the bridge is up and the launcher's welcome lists {@code link}
     */
    public void onAsk(long now, boolean projects, String name, boolean vouches) {
        if (!projects || asking) {
            return;
        }
        if (!vouches || name == null || name.isBlank()) {
            out.toServer(LinkMessages.none());
            return;
        }
        out.toServer(LinkMessages.pending());
        asking = true;
        askedAt = now;
        askedName = name;
        out.toLauncher(Protocol.linkWant(name));
    }

    /**
     * The launcher's permit ({@code link-offer}). Handed on only to the ask it answers, only for the account the
     * game plays as and only while it lives long enough; otherwise the server hears {@code none}.
     */
    public void onOffer(String nonce, String name, long expiresAtSeconds, long wallMs) {
        if (!asking) {
            // Nobody asked: the permit is not kept.
            return;
        }
        String asked = askedName;
        asking = false;
        askedName = null;
        boolean good = LinkMessages.nonce(nonce) && name != null && name.equalsIgnoreCase(asked)
            && expiresAtSeconds * 1000L - wallMs > PERMIT_MARGIN_MS;
        out.toServer(good ? LinkMessages.intent(nonce) : LinkMessages.none());
    }

    /** {@code link-clear}: the launcher has no permit (or the bridge went): an open ask hears {@code none}. */
    public void onClear() {
        if (asking) {
            asking = false;
            askedName = null;
            out.toServer(LinkMessages.none());
        }
    }

    /**
     * The server's question, for the screen once it is free. One at a time, only from Projects; it lives
     * {@link #SHOW_WAIT_MS} while another screen stays open.
     */
    public void onQuestion(long now, LinkMessages.Question question, boolean projects) {
        if (!projects || question == null || shown != null || waiting != null) {
            return;
        }
        waiting = question;
        waitingSince = now;
        // The screen counts as free only from now on (an earlier question's wait says nothing about this one).
        free = false;
    }

    /** Every tick: the launcher's answer and a waiting question run out. */
    public void tick(long now) {
        if (asking && now - askedAt >= LAUNCHER_WAIT_MS) {
            onClear();
        }
        if (waiting != null && now - waitingSince >= SHOW_WAIT_MS) {
            // Never found a free screen: "not now", as Escape.
            waiting = null;
        }
    }

    /**
     * The question to put on screen now, once (null otherwise): the game shows no other screen, and has shown
     * none for {@link #SETTLE_MS}.
     */
    public LinkMessages.Question takeQuestion(long now, boolean screenFree) {
        if (!screenFree) {
            free = false;
            return null;
        }
        if (!free) {
            free = true;
            freeSince = now;
        }
        if (waiting == null || now - freeSince < SETTLE_MS) {
            return null;
        }
        shown = waiting;
        waiting = null;
        return shown;
    }

    /** The player pressed "Evet, eşle" ({@code yes}) or "Hayır" on the question on screen. */
    public void answer(LinkMessages.Question question, boolean yes) {
        if (question == null || question != shown) {
            return;
        }
        shown = null;
        out.toServer(LinkMessages.answer(question.token(), yes));
    }

    /** Escape or closing: "not now", nothing goes (the server lets the question run out). */
    public void dismissed(LinkMessages.Question question) {
        if (question == shown) {
            shown = null;
        }
    }

    /** A question waits for a free screen (the game looks at its screens only then). */
    public boolean waitingForScreen() {
        return waiting != null;
    }

    /** The question on screen, if any (the game closes it when it leaves the server). */
    public LinkMessages.Question shown() {
        return shown;
    }

    /** The game left the server (or its proxy moved it to another one): that connection's ask and question end. */
    public void onLeave() {
        asking = false;
        askedName = null;
        waiting = null;
        shown = null;
        free = false;
    }
}
