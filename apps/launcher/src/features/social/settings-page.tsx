// "Arkadaş ayarları": profile picture (bundled pictures, the Discord photo, the Minecraft head),
// privacy (the crew tag worn before the name, TEAM6; with crews, "Birlikte oynanan süre": kept on
// this computer only, deleted when turned off or with "Süreleri sil", TEAM5), world sharing's
// privacy by its name (HOST1), favourite
// notifications, the settings backup, sessions, blocked players, sign-out and "Verilerimi sil". The
// same component shows in the panel and in Settings → Arkadaşlar. With chat, "Sohbet" holds the
// chat's text size (CHAT-PROD), for players who do not use Ctrl + wheel.
import { Desktop, SignOut, Trash } from "@phosphor-icons/react";
import { Button, Dialog, Select, Skeleton, Slider, Switch } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { announce, useAccounts } from "@/features/accounts/use-accounts";
import { AvatarPicker } from "@/features/avatars/avatar-picker";
import { useHostUi } from "@/features/host/store";
import { usePlayInstance } from "@/features/settings/graphics-section";
import { api } from "@/lib/api";
import type { SocialMe, SocialSession } from "@/lib/api/social-types";
import { formatPercent, formatRelative } from "@/lib/format";
import { attempt } from "./actions";
import { wearTag } from "./badge-actions";
import { tagChoices } from "./badges";
import { useChatTextSize } from "./chat/text-size";
import { DEFAULT_TEXT_SIZE, TEXT_SIZES, zoomOf } from "./chat/text-zoom";
import { CrewNotifyGroup } from "./crew-notify";
import { dayMonth, limitOf } from "./limits";
import { rise, useSocialMotion } from "./motion";
import { NameWithMark } from "./person-mark";
import { SocialAvatar, SocialFace } from "./social-face";
import { useSocialPrefs, useSocialUi } from "./store";
import { useSwitchNow } from "./switch-now";

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

/**
 * "Birlikte oynanan süre": on or off (off deletes what was kept), and "Süreleri sil", asked once in
 * place. The switch moves at the click and follows the launcher's view once it shows the same.
 */
function TogetherSetting() {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const kept = useSocialUi((s) => s.view.together);
  const save = useCallback((next: boolean) => attempt(() => api.social.setTogether(next)), []);
  const [on, change] = useSwitchNow(kept, save);
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState(false);
  const toggle = (next: boolean) => {
    setAsking(false);
    change(next);
  };
  const clear = async () => {
    setBusy(true);
    const done = await attempt(() => api.social.clearTogether());
    setBusy(false);
    setAsking(false);
    if (done !== undefined) announce(t("crewWave2.together.cleared"));
  };
  return (
    <div>
      <Switch
        checked={on}
        onCheckedChange={toggle}
        label={t("crewWave2.together.setting")}
        description={t("crewWave2.together.settingNote")}
      />
      <AnimatePresence mode="popLayout" initial={false}>
        {on && asking ? (
          <motion.div
            key="ask"
            className="mt-2.5 flex items-center gap-2"
            {...rise(reduce, 4, 0.2)}
          >
            <p className="min-w-0 flex-1 text-[13px] text-bone-100">
              {t("crewWave2.together.clearAsk")}
            </p>
            <Button size="sm" variant="ghost" onClick={() => setAsking(false)}>
              {t("teams.cancel")}
            </Button>
            <Button size="sm" variant="danger" disabled={busy} onClick={() => void clear()}>
              {t("crewWave2.together.clear")}
            </Button>
          </motion.div>
        ) : on ? (
          <motion.div key="clear" className="mt-1.5" {...rise(reduce, 4, 0.2)}>
            <Button
              size="sm"
              variant="ghost"
              icon={<Trash size={14} />}
              onClick={() => setAsking(true)}
            >
              {t("crewWave2.together.clear")}
            </Button>
          </motion.div>
        ) : null}
      </AnimatePresence>
    </div>
  );
}

/** "Ekip etiketi" (TEAM6): the crew whose tag the player wears before their name, or none; who
 * sees it under it. Crews whose tag cannot be worn now are left out (the crew menu says why). */
function TagSetting() {
  const { t } = useTranslation();
  const on = useSocialUi((s) => s.view.crew.badges);
  const crews = useSocialUi((s) => s.view.crews);
  const worn = useSocialUi((s) =>
    s.badgePending !== undefined ? s.badgePending : (s.view.me?.badge?.crew ?? null),
  );
  if (!on || crews.length === 0) return null;
  const choices = tagChoices(crews, Date.now()).filter(
    (choice) => choice.blocked === null || choice.crew.id === worn,
  );
  return (
    <div>
      <div className="flex items-center gap-3">
        <span className="min-w-0 flex-1 text-[13px] font-medium text-bone-100">
          {t("team6.choose")}
        </span>
        <Select
          className="w-[200px]"
          label={t("team6.choose")}
          value={worn ?? NO_TAG}
          onValueChange={(value) => void wearTag(value === NO_TAG ? null : value)}
          options={[
            { value: NO_TAG, label: t("team6.none") },
            ...choices.map(({ crew, main }) => ({
              value: crew.id,
              label: crew.name,
              hint: main ? t("crews.main") : undefined,
            })),
          ]}
        />
      </div>
      <p className="mt-1 text-[12px] leading-snug text-ash-400">{t("team6.whoSees")}</p>
    </div>
  );
}

/** "Yok" in the tag picker (Radix selects take no empty value). */
const NO_TAG = "none";

type PrivacyKey = "shareServer" | "shareLastSeen" | "shareRead" | "shareTyping";

/** One privacy switch: it turns at the click, the launcher sends it (SOC-LAG). */
function PrivacySwitch({
  field,
  kept,
  label,
  description,
}: {
  field: PrivacyKey;
  kept: boolean;
  label: string;
  description?: string;
}) {
  const save = useCallback(
    (on: boolean) => {
      const patch: Partial<Record<PrivacyKey, boolean>> = {};
      patch[field] = on;
      return attempt(() => api.social.setPrivacy(patch));
    },
    [field],
  );
  const [checked, change] = useSwitchNow(kept, save);
  return (
    <Switch checked={checked} onCheckedChange={change} label={label} description={description} />
  );
}

function PrivacySection({ me }: { me: SocialMe }) {
  const { t } = useTranslation();
  const crews = useSocialUi((s) => s.view.teams);
  // CHAT2: "Görüldü bilgisi" and "Yazıyor bilgisi", both ways (off: neither shown nor seen),
  // while the service has them.
  const extras = useSocialUi((s) => s.view.chat?.extras);
  // "Görüldü" and "Yazıyor" bilgisi side by side share one note under the second.
  const bothShares =
    Boolean(extras) && me.privacy.shareRead !== undefined && me.privacy.shareTyping !== undefined;
  return (
    <Section title={t("social.settings.privacy")}>
      <div className="space-y-3.5">
        <TagSetting />
        <PrivacySwitch
          field="shareServer"
          kept={me.privacy.shareServer}
          label={t("social.settings.shareServer")}
        />
        <PrivacySwitch
          field="shareLastSeen"
          kept={me.privacy.shareLastSeen}
          label={t("social.settings.shareLastSeen")}
        />
        {extras && me.privacy.shareRead !== undefined ? (
          <PrivacySwitch
            field="shareRead"
            kept={me.privacy.shareRead}
            label={t("chat2.privacy.seen")}
            description={bothShares ? undefined : t("chat2.privacy.note")}
          />
        ) : null}
        {extras && me.privacy.shareTyping !== undefined ? (
          <PrivacySwitch
            field="shareTyping"
            kept={me.privacy.shareTyping}
            label={t("chat2.privacy.typing")}
            description={t(bothShares ? "chat2.privacy.both" : "chat2.privacy.note")}
          />
        ) : null}
        {crews ? <TogetherSetting /> : null}
      </div>
    </Section>
  );
}

/**
 * Two bubbles at the chosen text size (the conversation is not on screen here), at the bottom like
 * a conversation's newest messages. The box is as tall as the largest size needs, with room to
 * spare: nothing below it moves while the slider moves.
 */
function TextSizeSample({ size }: { size: number }) {
  const { t } = useTranslation();
  return (
    <div
      aria-hidden
      className="mt-3 flex h-[144px] flex-col justify-end overflow-hidden pb-3 rounded-[var(--radius-panel)] bg-obsidian-950/45 px-3 ring-1 ring-white/8"
    >
      <div className="flex flex-col gap-1" style={{ zoom: zoomOf(size) }}>
        <span className="max-w-[85%] self-start rounded-[10px] bg-obsidian-800/85 px-3 py-[7px] text-[13.5px] leading-[1.45] text-bone-100 ring-1 ring-white/7 ring-inset">
          {t("chatProd.settings.sampleTheirs")}
        </span>
        <span className="max-w-[85%] self-end rounded-[10px] bg-[color-mix(in_oklab,var(--color-gold-500)_17%,var(--color-obsidian-800))] px-3 py-[7px] text-[13.5px] leading-[1.45] text-bone-50 ring-1 ring-gold-400/20 ring-inset">
          {t("chatProd.settings.sampleMine")}
        </span>
      </div>
    </div>
  );
}

/** "Sohbet" (with chat): the chat's text size, the same one Ctrl + wheel and Ctrl + "+"/"-" set. */
function ChatSection() {
  const { t } = useTranslation();
  const chat = useSocialUi((s) => s.view.chat !== undefined);
  const size = useChatTextSize((s) => s.size);
  const setSize = useChatTextSize((s) => s.setSize);
  if (!chat) return null;
  const sizeAt = (index: number) => TEXT_SIZES[index] ?? DEFAULT_TEXT_SIZE;
  return (
    <Section title={t("chatProd.settings.title")}>
      <Slider
        label={t("chatProd.zoom.label")}
        value={TEXT_SIZES.indexOf(size)}
        onValueChange={(index) => setSize(sizeAt(index))}
        min={0}
        max={TEXT_SIZES.length - 1}
        step={1}
        format={(index) => formatPercent(sizeAt(index))}
      />
      <TextSizeSample size={size} />
    </Section>
  );
}

/**
 * "Dünyanı arkadaşlarına aç" by its name, while world sharing is on (HOST1): what it keeps to
 * itself. The invite window says the first half once per computer; here it can be found again.
 */
function WorldsSection() {
  const { t } = useTranslation();
  const on = useHostUi((s) => s.view.enabled);
  const navigate = useNavigate();
  const instances = useQuery({
    queryKey: ["instances"],
    queryFn: () => api.instances(),
    enabled: on,
  });
  // Oyna's install, as the command bar's "Dünyanı arkadaşlarına aç" (Ctrl+K) goes to its Worlds tab.
  const { instance } = usePlayInstance(instances.data ?? []);
  if (!on) return null;
  return (
    <Section title={t("small9.settings.worlds")}>
      <p className="max-w-[60ch] text-[13px] leading-relaxed text-ash-300 [text-wrap:pretty]">
        {t("small9.settings.worldsPrivacy")}
      </p>
      {instance ? (
        <Button
          size="sm"
          variant="secondary"
          className="mt-3"
          onClick={() => {
            useSocialUi.getState().closePanel();
            void navigate({
              to: "/library/$instanceId",
              params: { instanceId: instance.id },
              search: { tab: "worlds" },
            });
          }}
        >
          {t("small9.settings.worldsGo")}
        </Button>
      ) : null}
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
  // Two groups that say whose, so their switches say only when (CHAT-PROD leftovers).
  const groupTitle = "text-[13.5px] font-semibold text-bone-50";
  return (
    <Section title={t("social.settings.notifications")}>
      <p className={groupTitle}>{t("social.settings.favorites")}</p>
      <div className="mt-3.5 space-y-3.5">
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
      </div>
      {crews ? (
        <div className="mt-5">
          <CrewNotifyGroup title={t("social.settings.crewGroup")} titleClassName={groupTitle} />
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
              <span className="min-w-0 flex-1">
                <NameWithMark
                  name={user.name}
                  mark={user.mark}
                  className="text-[14px] text-bone-100"
                />
              </span>
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
      <ChatSection />
      <WorldsSection />
      <NotificationSection />
      <BackupSection me={me} />
      <SessionsSection />
      <BlockedSection />
      <AccountSection />
    </div>
  );
}
