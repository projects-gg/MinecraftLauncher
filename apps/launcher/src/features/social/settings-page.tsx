// "Arkadaş ayarları": profile picture (bundled pictures, the Discord photo, the Minecraft head),
// privacy, favourite notifications, the settings backup, sessions, blocked players, sign-out and
// "Verilerimi sil". The same component shows in the panel and in Settings → Arkadaşlar.
import { Desktop, SignOut, Trash } from "@phosphor-icons/react";
import { Button, Dialog, Skeleton, Switch } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import { announce, useAccounts } from "@/features/accounts/use-accounts";
import { AvatarPicker } from "@/features/avatars/avatar-picker";
import { api } from "@/lib/api";
import type { SocialMe, SocialSession } from "@/lib/api/social-types";
import { formatRelative } from "@/lib/format";
import { attempt } from "./actions";
import { CrewNotifyGroup } from "./crew-notify";
import { dayMonth, limitOf } from "./limits";
import { rise, useSocialMotion } from "./motion";
import { SocialAvatar, SocialFace } from "./social-face";
import { useSocialPrefs, useSocialUi } from "./store";

const SESSIONS_KEY = ["social-sessions"] as const;

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="border-b border-white/7 py-5 first:pt-2 last:border-b-0">
      <h3 className="mb-3 font-display text-[15px] font-semibold text-bone-50">{title}</h3>
      {children}
    </section>
  );
}

/** The texture hash of the player's Minecraft skin (active Microsoft account first). */
function useMinecraftTexture(): string | null {
  const { data: accounts = [] } = useAccounts();
  const microsoft =
    accounts.find((a) => a.active && a.kind === "microsoft") ??
    accounts.find((a) => a.kind === "microsoft");
  const { data } = useQuery({
    queryKey: ["skin-appearance", microsoft?.id],
    queryFn: () => api.skins.appearance(microsoft?.id ?? ""),
    enabled: Boolean(microsoft),
    staleTime: 10 * 60_000,
  });
  const match = data?.skin?.url.match(/\/texture\/([0-9a-f]{40,64})$/);
  return match?.[1] ?? null;
}

function PictureSection({ me }: { me: SocialMe }) {
  const { t } = useTranslation();
  const texture = useMinecraftTexture();
  const [pending, setPending] = useState<string | null>(null);
  const leading = [
    {
      id: "discord",
      label: t("social.settings.discordPhoto"),
      face: (size: number) => (
        <SocialFace
          source={{ name: me.name, avatar: "discord", avatarUrl: me.discordAvatarUrl }}
          size={size}
        />
      ),
    },
    ...(texture
      ? [
          {
            id: `minecraft:${texture}`,
            label: t("social.settings.minecraftHead"),
            face: (size: number) => (
              <SocialFace source={{ name: me.name, avatar: `minecraft:${texture}` }} size={size} />
            ),
          },
        ]
      : []),
  ];
  return (
    <Section title={t("social.settings.profile")}>
      <AvatarPicker
        value={pending ?? me.avatar}
        label={t("social.settings.profile")}
        columns={7}
        size={44}
        leading={leading}
        onSelect={(id) => {
          if (!id || id === me.avatar) return;
          setPending(id);
          void attempt(() => api.social.setAvatar(id)).then((done) => {
            setPending(null);
            if (done !== undefined) announce(t("social.settings.pictureChanged"));
          });
        }}
      />
    </Section>
  );
}

function PrivacySection({ me }: { me: SocialMe }) {
  const { t } = useTranslation();
  const set = (change: { shareServer?: boolean; shareLastSeen?: boolean }) =>
    void attempt(() => api.social.setPrivacy(change));
  return (
    <Section title={t("social.settings.privacy")}>
      <div className="space-y-3.5">
        <Switch
          checked={me.privacy.shareServer}
          onCheckedChange={(shareServer) => set({ shareServer })}
          label={t("social.settings.shareServer")}
        />
        <Switch
          checked={me.privacy.shareLastSeen}
          onCheckedChange={(shareLastSeen) => set({ shareLastSeen })}
          label={t("social.settings.shareLastSeen")}
        />
      </div>
    </Section>
  );
}

function NotificationSection() {
  const { t } = useTranslation();
  const online = useSocialPrefs((s) => s.notifyFavoriteOnline);
  const projects = useSocialPrefs((s) => s.notifyFavoriteProjects);
  const set = useSocialPrefs((s) => s.set);
  // The crew's own group, while the player is in one (the crew page's bell shows it too); with
  // several crews it mutes all of them at once.
  const crews = useSocialUi((s) => s.view.teams && s.view.crews.length > 0);
  // Crew chat only counts unread unless this is on (CHAT1).
  const chat = useSocialUi((s) => s.view.chat !== undefined);
  const crewChat = useSocialPrefs((s) => s.notifyCrewChat);
  return (
    <Section title={t("social.settings.notifications")}>
      <div className="space-y-3.5">
        <Switch
          checked={online}
          onCheckedChange={(on) => set("notifyFavoriteOnline", on)}
          label={t("social.settings.favoriteOnline")}
        />
        <Switch
          checked={projects}
          onCheckedChange={(on) => set("notifyFavoriteProjects", on)}
          label={t("social.settings.favoriteProjects")}
        />
        {chat && crews ? (
          <Switch
            checked={crewChat}
            onCheckedChange={(on) => set("notifyCrewChat", on)}
            label={t("chat.settings.crewChat")}
          />
        ) : null}
      </div>
      {crews ? (
        <div className="mt-5">
          <CrewNotifyGroup titleClassName="text-[13.5px] font-semibold text-bone-50" />
        </div>
      ) : null}
    </Section>
  );
}

function BackupSection({ me }: { me: SocialMe }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const enabled = useSocialPrefs((s) => s.backup);
  const set = useSocialPrefs((s) => s.set);
  // A new Discord account keeps no backup (SPAM1); storage protection makes uploads wait.
  const limit = limitOf(me, "settingsBackup", Date.now());
  const waits = useSocialUi(
    (s) => s.view.backupWaitsUntil && s.view.backupWaitsUntil * 1000 > Date.now(),
  );
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState(false);
  const turnOff = async () => {
    setBusy(true);
    const done = me.backup ? await attempt(() => api.social.deleteBackup()) : true;
    setBusy(false);
    setAsking(false);
    if (done === undefined) return;
    set("backup", false);
    set("backupFingerprint", null);
  };
  return (
    <Section title={t("social.settings.backup")}>
      <Switch
        checked={enabled && !asking && !limit}
        disabled={Boolean(limit)}
        onCheckedChange={(on) => {
          if (on) {
            set("backup", true);
          } else {
            setAsking(true);
          }
        }}
        label={t("social.settings.backupToggle")}
        description={
          limit
            ? t("safety.limited.backup", { date: dayMonth(limit.until) })
            : enabled && waits
              ? t("safety.backupWaits")
              : enabled && me.backup
                ? t("social.settings.backupWhen", {
                    when: formatRelative(new Date(me.backup.updatedAt * 1000).toISOString()),
                  })
                : undefined
        }
      />
      <AnimatePresence initial={false}>
        {asking ? (
          <motion.div
            key="ask"
            className="mt-3 flex items-center gap-2 rounded-md bg-flare-400/[0.08] px-3 py-2.5 ring-1 ring-flare-400/20"
            {...rise(reduce, 4, 0.2)}
          >
            <p className="min-w-0 flex-1 text-[13px] text-bone-100">
              {t("social.settings.backupOffAsk")}
            </p>
            <Button size="sm" variant="ghost" onClick={() => setAsking(false)}>
              {t("social.actions.cancel")}
            </Button>
            <Button size="sm" variant="danger" disabled={busy} onClick={() => void turnOff()}>
              {t("social.settings.backupOff")}
            </Button>
          </motion.div>
        ) : null}
      </AnimatePresence>
    </Section>
  );
}

function SessionRow({ session, onRevoke }: { session: SocialSession; onRevoke: () => void }) {
  const { t } = useTranslation();
  return (
    <div className="flex items-center gap-3 py-2">
      <span className="flex size-9 shrink-0 items-center justify-center rounded-lg bg-white/[0.05] text-ash-300">
        <Desktop size={18} aria-hidden />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-[14px] font-medium text-bone-50">
          {session.device}
        </span>
        <span className="block truncate text-[12.5px] text-ash-400">
          {session.current
            ? t("social.settings.thisDevice")
            : t("social.settings.lastUsed", {
                when: formatRelative(`${session.lastUsedOn}T12:00:00Z`),
              })}
        </span>
      </span>
      {session.current ? null : (
        <Button size="sm" variant="ghost" onClick={onRevoke}>
          {t("social.settings.revoke")}
        </Button>
      )}
    </div>
  );
}

function SessionsSection() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { data, isPending } = useQuery({
    queryKey: SESSIONS_KEY,
    queryFn: () => api.social.sessions(),
    staleTime: 30_000,
    retry: false,
  });
  const others = data?.filter((s) => !s.current) ?? [];
  const revoke = (id: string) =>
    void attempt(() => api.social.revokeSession(id)).then((done) => {
      if (done === undefined) return;
      queryClient.setQueryData<SocialSession[]>(SESSIONS_KEY, (list) =>
        list?.filter((s) => s.id !== id),
      );
    });
  const revokeOthers = () =>
    void attempt(() => api.social.revokeOtherSessions()).then((count) => {
      if (count === undefined) return;
      announce(t("social.settings.revokedOthers", { count }));
      queryClient.setQueryData<SocialSession[]>(SESSIONS_KEY, (list) =>
        list?.filter((s) => s.current),
      );
    });
  return (
    <Section title={t("social.settings.sessions")}>
      {isPending ? (
        <div aria-hidden className="space-y-3">
          {[0, 1].map((i) => (
            <div key={i} className="flex items-center gap-3">
              <Skeleton className="size-9 rounded-lg" />
              <div className="flex-1 space-y-1.5">
                <Skeleton className="h-3.5 w-32" />
                <Skeleton className="h-3 w-24" />
              </div>
            </div>
          ))}
        </div>
      ) : (
        <>
          <div className="divide-y divide-white/6">
            {(data ?? []).map((session) => (
              <SessionRow key={session.id} session={session} onRevoke={() => revoke(session.id)} />
            ))}
          </div>
          {others.length ? (
            <Button size="sm" variant="secondary" className="mt-3" onClick={revokeOthers}>
              {t("social.settings.revokeOthers")}
            </Button>
          ) : data ? (
            <p className="mt-1 text-[12.5px] text-ash-400">
              {t("social.settings.noOtherSessions")}
            </p>
          ) : null}
        </>
      )}
    </Section>
  );
}

function BlockedSection() {
  const { t } = useTranslation();
  const blocked = useSocialUi((s) => s.view.blocked);
  return (
    <Section title={t("social.settings.blocked")}>
      {blocked.length ? (
        <div className="divide-y divide-white/6">
          {blocked.map((user) => (
            <div key={user.id} className="flex items-center gap-3 py-2">
              <SocialAvatar source={user} size={32} />
              <span className="min-w-0 flex-1 truncate text-[14px] text-bone-100">{user.name}</span>
              <Button
                size="sm"
                variant="ghost"
                onClick={() =>
                  void attempt(() => api.social.unblock(user.id)).then((done) => {
                    if (done !== undefined)
                      announce(t("social.actions.unblocked", { name: user.name }));
                  })
                }
              >
                {t("social.actions.unblock")}
              </Button>
            </div>
          ))}
        </div>
      ) : (
        <p className="text-[13px] text-ash-400">{t("social.settings.blockedEmpty")}</p>
      )}
    </Section>
  );
}

function AccountSection() {
  const { t } = useTranslation();
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState(false);
  const goTo = useSocialUi((s) => s.goTo);
  const remove = async () => {
    setBusy(true);
    const done = await attempt(() => api.social.deleteAccount());
    setBusy(false);
    if (done === undefined) return;
    setAsking(false);
    goTo("list");
    announce(t("social.settings.deleted"));
  };
  return (
    <Section title={t("social.settings.account")}>
      <div className="flex flex-wrap gap-2">
        <Button
          variant="secondary"
          icon={<SignOut size={16} />}
          onClick={() => {
            goTo("list");
            void attempt(() => api.social.signOut());
          }}
        >
          {t("social.settings.signOut")}
        </Button>
        <Button variant="danger" icon={<Trash size={16} />} onClick={() => setAsking(true)}>
          {t("social.settings.deleteData")}
        </Button>
      </div>
      <Dialog
        open={asking}
        onOpenChange={(open) => !busy && setAsking(open)}
        title={t("social.settings.deleteTitle")}
        closeLabel={t("common.close")}
        width={440}
        dismissible={!busy}
        footer={
          <>
            <Button variant="ghost" disabled={busy} onClick={() => setAsking(false)}>
              {t("social.actions.cancel")}
            </Button>
            <Button variant="danger" disabled={busy} onClick={() => void remove()}>
              {t("social.settings.deleteConfirm")}
            </Button>
          </>
        }
      >
        <p className="text-[14px] leading-relaxed text-bone-100">
          {t("social.settings.deleteBody")}
        </p>
      </Dialog>
    </Section>
  );
}

export function SocialSettings() {
  const me = useSocialUi((s) => s.view.me);
  if (!me) return null;
  return (
    <div>
      <PictureSection me={me} />
      <PrivacySection me={me} />
      <NotificationSection />
      <BackupSection me={me} />
      <SessionsSection />
      <BlockedSection />
      <AccountSection />
    </div>
  );
}
