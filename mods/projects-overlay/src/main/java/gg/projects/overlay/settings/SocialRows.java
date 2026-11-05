package gg.projects.overlay.settings;

import gg.projects.overlay.state.OverlayState;
import java.util.ArrayList;
import java.util.List;

/**
 * OVL-SOC2: the settings page's rows that follow what the launcher said. "Oyunda gösterilecek kartlar": the card kinds
 * a launcher that keeps its own notifications takes over when the player turns them off here (each one is in
 * {@link OverlaySettings#offGroups()} when off, so the section's help line holds). "Arkadaş bildirimleri": the
 * launcher's own notification switches, shared with it. Pure, so the tests can say which rows show for what.
 */
public final class SocialRows {
    /**
     * The card kinds under "Oyunda gösterilecek kartlar", in the page's order: the older three first, then the newer
     * groups ({@link OverlaySettings#CARD_GROUPS}). "Diğer bildirimler" is not one: the launcher does not take it.
     */
    public static final List<String> CARDS = List.of("message", "call", "invite", "request", "online", "plan",
        "missed");

    /** One of the launcher's notification switches: its key in the frames and its row's words. */
    public record Notify(String key, String words) {
    }

    private SocialRows() {
    }

    /** The words key of a card kind's row ({@code settings.toasts.<group>}). */
    public static String words(String group) {
        return "settings.toasts." + group;
    }

    /** Whether cards of {@code group} (one of {@link #CARDS}) show in game. */
    public static boolean shows(OverlaySettings s, String group) {
        return switch (group) {
            case "message" -> s.messageToasts();
            case "call" -> s.callToasts();
            case "invite" -> s.inviteToasts();
            default -> s.showsCards(group);
        };
    }

    /** {@code s} with cards of {@code group} (one of {@link #CARDS}) shown in game ({@code on}) or not. */
    public static OverlaySettings show(OverlaySettings s, String group, boolean on) {
        return switch (group) {
            case "message" -> s.withMessageToasts(on);
            case "call" -> s.withCallToasts(on);
            case "invite" -> s.withInviteToasts(on);
            default -> s.withCards(group, on);
        };
    }

    /**
     * The launcher's notification switches the page shows: the favourites' always, the crew calls and join requests
     * while the launcher takes the crews' frames ({@code crews}), the crew chat while the overlay has chat.
     */
    public static List<Notify> notify(boolean crews, boolean chat) {
        List<Notify> out = new ArrayList<>(5);
        out.add(new Notify("favoritesOnline", "settings.notify.favorites_online"));
        out.add(new Notify("favoritesProjects", "settings.notify.favorites_projects"));
        if (crews) {
            out.add(new Notify("crewCalls", "settings.notify.crew_calls"));
        }
        if (chat) {
            out.add(new Notify("crewChat", "settings.notify.crew_chat"));
        }
        if (crews) {
            out.add(new Notify("joinRequests", "settings.notify.join_requests"));
        }
        return out;
    }

    /** Whether {@code key}'s switch is on in {@code prefs} (off while the launcher has not said them). */
    public static boolean on(OverlayState.Prefs prefs, String key) {
        if (prefs == null) {
            return false;
        }
        return switch (key) {
            case "favoritesOnline" -> prefs.favoritesOnline();
            case "favoritesProjects" -> prefs.favoritesProjects();
            case "crewCalls" -> prefs.crewCalls();
            case "crewChat" -> prefs.crewChat();
            case "joinRequests" -> prefs.joinRequests();
            default -> false;
        };
    }
}
