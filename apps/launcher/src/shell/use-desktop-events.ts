import { toast } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { useAfkWatch } from "@/features/afk/use-afk-watch";
import { officialStatusQuery, playTarget, useOfficialOffer } from "@/features/library/official";
import { useSocialUi } from "@/features/social/store";
import { api, type DeepLink } from "@/lib/api";
import { readStartOnBoot } from "@/lib/autostart";
import i18n from "@/lib/i18n";
import { onAppLink } from "@/lib/links";
import { followAdoptedGame, useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { useCloseButton } from "./close-button";

/** A desktop shortcut (`projects://play/<instance>`) starts that instance like OYNA would. */
async function playInstance(id: string) {
  const instances = await api.instances();
  if (!instances.some((i) => i.id === id)) {
    toast(i18n.t("shortcut.missing"));
    return;
  }
  usePrefs.getState().selectInstance(id);
  await useLaunch.getState().play(id);
}

/**
 * Plays what OYNA would (the selected instance, else the first). With nothing to play it opens
 * OYNA's offer ("Hiç kurulum yok", "Kur ve oyna"): nothing is made behind the player's back (LIB1).
 */
async function playPreferred() {
  const prefs = usePrefs.getState();
  const instances = await api.instances();
  const id = playTarget(instances, prefs.selectedInstanceId)?.id;
  if (!id) {
    useOfficialOffer.getState().show({ kind: "play" });
    return;
  }
  prefs.selectInstance(id);
  await useLaunch.getState().play(id);
}

/**
 * Desktop glue: content published in the background, the official pack appearing, tray actions,
 * `projects://` links, the close button (shell/close-button.ts) and the notice when an AFK
 * account stops for good.
 */
export function useDesktopEvents() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const onboarded = usePrefs((s) => s.onboarded);
  useAfkWatch();
  useCloseButton();

  useEffect(
    () =>
      api.onContentChanged(() => {
        void queryClient.invalidateQueries({ queryKey: ["home-content"] });
        void queryClient.invalidateQueries({ queryKey: ["official-status"] });
        // The version lists mark the recommended Minecraft version of the signed config.
        void queryClient.invalidateQueries({ queryKey: ["mc-versions"] });
        void queryClient.invalidateQueries({ queryKey: ["instance-versions"] });
        // Ready-made sets: a new catalog may bring an update to a set an instance has.
        void queryClient.invalidateQueries({ queryKey: ["mod-sets"] });
        void queryClient.invalidateQueries({ queryKey: ["mod-sets-instance"] });
      }),
    [queryClient],
  );

  // The home screen's offline strip follows the content servers' reachability; when the system
  // reports the network gone or back, they are asked again right away.
  useEffect(
    () => api.onContentStatus((status) => queryClient.setQueryData(["content-status"], status)),
    [queryClient],
  );
  useEffect(() => {
    const recheck = () => void api.contentRecheck();
    window.addEventListener("online", recheck);
    window.addEventListener("offline", recheck);
    return () => {
      window.removeEventListener("online", recheck);
      window.removeEventListener("offline", recheck);
    };
  }, []);

  // When the server publishes its pack, the Projects instance starts following it. The welcome
  // flow creates that instance itself and says so, so nothing is announced before it ends (its
  // status query fills this cache too). Once the player deleted it, it stays deleted (LIB1): the
  // library offers it again instead.
  const { data: official } = useQuery({ ...officialStatusQuery, enabled: onboarded });
  useEffect(() => {
    const pack = official?.pack;
    if (!onboarded || !pack || official.instanceId || official.removed) return;
    void api
      .ensureOfficialInstance()
      .then((instance) => {
        if (!instance) return;
        usePrefs.getState().selectInstance(instance.id);
        void queryClient.invalidateQueries({ queryKey: ["instances"] });
        void queryClient.invalidateQueries({ queryKey: ["official-status"] });
        toast.success(t("official.added", { name: pack.name }));
      })
      .catch(() => {
        // Tried again on the next content change or start.
      });
  }, [official, onboarded, queryClient, t]);

  useEffect(() => {
    const open = (link: DeepLink) => {
      switch (link.kind) {
        case "home":
          void navigate({ to: "/" });
          break;
        case "afk":
          void navigate({ to: "/afk" });
          break;
        case "news":
          void navigate({ to: "/", search: { news: link.id } });
          break;
        case "instance":
          void navigate({ to: "/library/$instanceId", params: { instanceId: link.id } });
          break;
        case "play":
          void navigate({ to: "/" });
          void playPreferred();
          break;
        case "playInstance":
          void navigate({ to: "/" });
          void playInstance(link.id);
          break;
        case "friend":
          // projects://friend/<code> (QR page): the friends panel shows who it is.
          useSocialUi.getState().openPanel({ page: "add", code: link.code });
          break;
        case "crew":
          // projects://crew/<link> (a crew's invite link page): "Ekip kur ya da katıl" shows the
          // crew; nothing is sent until the player confirms.
          useSocialUi.getState().openPanel({ page: "team", link: link.link });
          break;
      }
    };
    const stop = api.onDeepLink(open);
    // The same links inside content (a news button, a banner, an event): lib/links.ts.
    const stopApp = onAppLink(open);
    void api.takePendingDeepLink().then((link) => {
      if (link) open(link);
    });
    return () => {
      stop();
      stopApp();
    };
  }, [navigate]);

  // The login item may have been changed from the OS settings.
  useEffect(() => {
    void readStartOnBoot().then((on) => {
      if (on !== null) usePrefs.getState().set("startOnBoot", on);
    });
  }, []);

  // A game the last start left running ("Kapansın") is "Oyunda" until it ends; then its play time
  // and the views that wait for a closed game are read again.
  useEffect(
    () =>
      followAdoptedGame((instanceId) => {
        void queryClient.invalidateQueries({ queryKey: ["instances"] });
        void queryClient.invalidateQueries({ queryKey: ["instance", instanceId] });
        void queryClient.invalidateQueries({ queryKey: ["game-running"] });
      }),
    [queryClient],
  );
}
