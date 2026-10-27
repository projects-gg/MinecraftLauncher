// The link parts of the Projects skin panel (skin1.md §13.2–13.3): the code box while the account
// is not linked, the card while the Discord account is linked to another game account, a
// skeleton while ProBot is asked, the quiet linked line under the editor, the Discord card while
// signed out, and the question before a link is removed (also used by the account center's row).
import { DotsThree, LinkSimple } from "@phosphor-icons/react";
import { Button, cn, Dialog, IconButton, Menu, PlayerHead, Skeleton, toast } from "@pl/ui";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import { DiscordMark } from "@/features/social/discord-button";
import { projectsSkinApi } from "@/lib/api/projects-skin-api";
import { LinkCode } from "./link-code";
import { refusalNote } from "./rules";
import { COMMAND_SLOT, CommandText, noteText } from "./text";

/** "Hesabını eşle": what to type in game, and the six cells. */
export function LinkCodeBox({
  accountId,
  disabled,
  retryAt,
  autoFocus,
  onSuccess,
  onLinked,
  onOther,
}: {
  accountId: string;
  disabled: boolean;
  retryAt: number | null;
  autoFocus?: boolean;
  onSuccess?: () => void;
  onLinked: (name: string) => void;
  onOther: (name: string) => void;
}) {
  const { t } = useTranslation();
  const ids = useId();
  return (
    <section
      aria-labelledby={`${ids}-title`}
      className="rounded-[var(--radius-panel)] bg-gold-500/[0.04] px-4 pt-3.5 pb-2 ring-1 ring-gold-400/25"
    >
      <h3 id={`${ids}-title`} className="text-[14px] font-medium text-bone-50">
        {t("projectsSkin.link.title")}
      </h3>
      <p id={`${ids}-body`} className="mt-0.5 mb-3 max-w-[52ch] text-[12.5px] text-ash-300">
        <CommandText text={t("projectsSkin.link.body", { command: COMMAND_SLOT })} />
      </p>
      <LinkCode
        accountId={accountId}
        disabled={disabled}
        retryAt={retryAt}
        autoFocus={autoFocus}
        describedBy={`${ids}-body`}
        onSuccess={onSuccess}
        onLinked={onLinked}
        onOther={onOther}
      />
    </section>
  );
}

/** "Discord hesabın Mehmet hesabına eşli." with "Bu hesabı eşle". */
export function OtherAccountCard({
  name,
  headUrl,
  disabled,
  onLinkThis,
}: {
  name: string;
  headUrl: string | null;
  disabled: boolean;
  onLinkThis: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div
      className={cn(
        "flex items-center gap-3 rounded-[var(--radius-panel)] bg-obsidian-950/35 px-4 py-3 ring-1 ring-white/7 transition-opacity duration-150",
        disabled && "opacity-55",
      )}
    >
      <PlayerHead name={name} src={headUrl} size={32} />
      <p className="min-w-0 flex-1 text-[13.5px] text-bone-100">
        {t("projectsSkin.link.other", { name })}
      </p>
      <Button size="sm" variant="secondary" disabled={disabled} onClick={onLinkThis}>
        {t("projectsSkin.link.linkThis")}
      </Button>
    </div>
  );
}

/** While ProBot is asked about the link: the linked line's shape. */
export function CheckingLine() {
  const { t } = useTranslation();
  return (
    <div role="status" className="flex h-[40px] items-center gap-3">
      <span className="sr-only">{t("projectsSkin.link.checking")}</span>
      <Skeleton className="size-4 rounded-[4px]" />
      <Skeleton className="h-3.5 w-48" />
    </div>
  );
}

/** "Discord eşlemesi · Ahmet", quiet, with "Eşlemeyi kaldır" behind "…". */
export function LinkLine({
  name,
  dim,
  onRemove,
}: {
  name: string;
  dim: boolean;
  onRemove: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div
      className={cn(
        "flex h-[40px] items-center gap-2.5 transition-opacity duration-150",
        dim && "opacity-55",
      )}
    >
      <LinkSimple size={16} weight="bold" aria-hidden className="shrink-0 text-ash-400" />
      <p className="min-w-0 flex-1 truncate text-[13px]">
        <span className="text-ash-300">{t("projectsSkin.link.label")}</span>
        <span aria-hidden className="px-1.5 text-ash-500">
          ·
        </span>
        <span className="font-medium text-bone-50" data-selectable>
          {name}
        </span>
      </p>
      <Menu
        items={[
          {
            label: t("projectsSkin.link.remove"),
            danger: true,
            disabled: dim,
            onSelect: onRemove,
          },
        ]}
        trigger={
          <IconButton
            size="sm"
            label={t("projectsSkin.link.more")}
            icon={<DotsThree size={18} weight="bold" />}
          />
        }
      />
    </div>
  );
}

/** Signed out of friends (or the session ended): the way in, through the friends panel. */
export function ConnectCard({ session, onConnect }: { session: boolean; onConnect: () => void }) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-1 flex-col items-center justify-center px-8 pb-10 text-center">
      <span
        aria-hidden
        className="chamfer flex size-12 items-center justify-center bg-obsidian-800 [--cut:8px]"
      >
        <DiscordMark height={17} />
      </span>
      <h3 className="mt-5 max-w-[30ch] font-display text-[17px] leading-tight font-semibold text-bone-50 [text-wrap:balance]">
        {session ? t("projectsSkin.connect.sessionTitle") : t("projectsSkin.connect.title")}
      </h3>
      {session ? null : (
        <p className="mt-2 max-w-[40ch] text-[13.5px] text-ash-300 [text-wrap:pretty]">
          {t("projectsSkin.connect.body")}
        </p>
      )}
      <Button
        className="mt-6"
        variant="secondary"
        icon={<DiscordMark height={12} />}
        onClick={onConnect}
      >
        {session ? t("projectsSkin.connect.again") : t("projectsSkin.connect.action")}
      </Button>
    </div>
  );
}

/**
 * "Eşleme kaldırılsın mı?" (or "Mehmet eşlemesi kaldırılsın mı?" before linking another account).
 * "Kaldır" sends the removal at once; its toast comes when ProBot is done.
 */
export function UnlinkDialog({
  open,
  movingFrom,
  onOpenChange,
}: {
  open: boolean;
  /** The game account whose link goes so another can be linked (the question names it). */
  movingFrom: string | null;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation();
  const remove = async () => {
    onOpenChange(false);
    try {
      const sent = await projectsSkinApi.unlink();
      if (sent.status === "refused") {
        toast.error(noteText(t, refusalNote(sent.refusal, Math.floor(Date.now() / 1000))));
      }
    } catch {
      toast.error(t("projectsSkin.note.linkFailed"));
    }
  };
  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title={
        movingFrom
          ? t("projectsSkin.link.moveTitle", { name: movingFrom })
          : t("projectsSkin.link.removeTitle")
      }
      description={t("projectsSkin.link.removeBody")}
      closeLabel={t("common.close")}
      width={440}
      footer={
        <>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            {t("projectsSkin.link.removeCancel")}
          </Button>
          <Button variant="danger" onClick={() => void remove()}>
            {t("projectsSkin.link.removeConfirm")}
          </Button>
        </>
      }
    />
  );
}
