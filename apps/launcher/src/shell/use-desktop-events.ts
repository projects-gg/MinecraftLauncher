import { toast } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { commands } from "@/bindings";
import { useSocialUi } from "@/features/social/store";
import { api, type DeepLink } from "@/lib/api";
import { notify } from "@/lib/attention";
import { readStartOnBoot } from "@/lib/autostart";
import i18n from "@/lib/i18n";
import { onAppLink } from "@/lib/links";
import { isTauri } from "@/lib/platform";
import { refreshWindowAway } from "@/lib/window-away";
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";

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

/** Plays the selected instance, falling back to the Projects instance. */
async function playPreferred() {
  const prefs = usePrefs.getState();
  const instances = await api.instances();
  let id = instances.some((i) => i.id === prefs.selectedInstanceId)
    ? prefs.selectedInstanceId
    : null;
  if (!id) {
    id = (await api.ensureOfficialInstance()).id;
    prefs.selectInstance(id);
  }
  await useLaunch.getState().play(id);
}

/**
 * Desktop glue: content published in the background, the official pack appearing, tray actions,
 * `projects://` links and the close button (AFK accounts keep running in the tray).
 */
export function useDesktopEvents() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const onboarded = usePrefs((s) => s.onboarded);

  useEffect(
    () =>
      api.onContentChanged(() => {
        void queryClient.invalidateQueries({ queryKey: ["home-content"] });
        void queryClient.invalidateQueries({ queryKey: ["official-status"] });
        // The version lists mark the recommended Minecraft version of the signed config.
        void queryClient.invalidateQueries({ queryKey: ["mc-versions"] });
        void queryClient.invalidateQueries({ queryKey: ["instance-versions"] });
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
  // status query fills this cache too).
  const { data: official } = useQuery({
    queryKey: ["official-status"],
    queryFn: () => api.officialStatus(),
    enabled: onboarded,
    staleTime: 5 * 60_000,
  });
  useEffect(() => {
    const pack = official?.pack;
    if (!onboarded || !pack || official.instanceId) return;
    void api
      .ensureOfficialInstance()
      .then((instance) => {
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

  useEffect(() => {
    if (!isTauri()) return;
    const win = getCurrentWindow();
    const unlisten = win.onCloseRequested(async (event) => {
      event.preventDefault();
      const afkActive = await commands.afkAnyActive();
      // A running game keeps its play time and crash help only while we keep watching it.
      const gameBusy = ["working", "launching", "running"].includes(
        useLaunch.getState().phase.kind,
      );
      const prefs = usePrefs.getState();
      if (!prefs.closeToTray && !afkActive && !gameBusy) {
        await api.quitApp();
        return;
      }
      await win.hide();
      // Hiding sends no window event: ambient motion stops on this answer.
      void refreshWindowAway();
      if (!prefs.trayHintShown) {
        prefs.set("trayHintShown", true);
        await notify(t("tray.hintTitle"), afkActive ? t("tray.hintAfk") : t("tray.hint"));
      }
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [t]);
}
