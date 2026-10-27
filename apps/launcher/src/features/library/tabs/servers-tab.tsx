import {
  Copy,
  DotsThreeVertical,
  LockSimple,
  PencilSimple,
  Play,
  Plus,
  Trash,
} from "@phosphor-icons/react";
import {
  Button,
  ContentIcon,
  ContextMenu,
  Dialog,
  Field,
  IconButton,
  Input,
  Menu,
  type MenuItem,
  SignalBars,
  type SignalState,
  toast,
  useMotionPrefs,
} from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { type FormEvent, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { errorCode, showError } from "@/features/content/jobs";
import {
  api,
  type InstanceDetail,
  type SavedServer,
  type SavedServerEntry,
  type ServerEdit,
} from "@/lib/api";
import { copyText } from "@/lib/clipboard";
import { formatCount } from "@/lib/format";
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { InlineNotice, TabEmpty } from "../parts";
import { parseServerAddress, serverRowKeys } from "../quick-play";
import { useGameRunning, useOwnGameOf } from "../use-game-running";

type Editing = { mode: "add" } | { mode: "edit"; index: number; server: SavedServer };

/** One server's ping, shared by its icon and its status line; asked again at most once a minute. */
function useServerPing(address: string) {
  return useQuery({
    queryKey: ["server-ping", address.trim().toLowerCase()],
    queryFn: () => api.library.pingServer(address),
    staleTime: 60_000,
    refetchOnWindowFocus: false,
    retry: false,
  });
}

/** The icon the game saved for the server, else the one its ping sends, else the server mark. */
function ServerIcon({ server }: { server: SavedServer }) {
  const ping = useServerPing(server.address);
  const src = server.iconUrl ?? ping.data?.iconUrl;
  return (
    <span className="chamfer flex size-11 shrink-0 items-center justify-center overflow-hidden bg-obsidian-800 [--cut:6px]">
      {src ? (
        <img src={src} alt="" draggable={false} className="size-full [image-rendering:pixelated]" />
      ) : (
        <ContentIcon kind="server" quiet />
      )}
    </span>
  );
}

/**
 * Players and reachability of one server. The bars sit at the slot's right edge, as in the game's
 * server list, and sweep while the ping is out; the words fade in beside them.
 */
function ServerStatusLine({ address }: { address: string }) {
  const { t } = useTranslation();
  const ping = useServerPing(address);
  const data = ping.data;
  const state: SignalState = data
    ? data.online
      ? "online"
      : "offline"
    : ping.isError
      ? "offline"
      : "pending";
  return (
    // Fixed width: the line below never shifts when the answer arrives.
    <span className="flex h-5 w-[172px] shrink-0 items-center justify-end gap-2 text-[12.5px] whitespace-nowrap text-ash-300 tabular-nums">
      <span className="relative flex min-w-0 justify-end">
        <AnimatePresence initial={false} mode="popLayout">
          {state === "pending" ? null : (
            <motion.span
              key={state}
              initial={{ opacity: 0 }}
              animate={{ opacity: 1, transition: { duration: 0.18 } }}
              exit={{ opacity: 0, transition: { duration: 0.1 } }}
            >
              {data?.online
                ? t("instance.servers.players", {
                    players: formatCount(data.players),
                    max: formatCount(data.maxPlayers),
                  })
                : t("instance.servers.unreachable")}
            </motion.span>
          )}
        </AnimatePresence>
      </span>
      <SignalBars state={state} latencyMs={data?.latencyMs} />
    </span>
  );
}

/**
 * The instance's in-game server list: each server's players, "Katıl" to start the game straight
 * into it, add, edit and remove (with "Geri al"). The list is the game's own `servers.dat`, so it
 * is not changed while the game runs (the game would write its copy back over it).
 */
export function ServersTab({ instance }: { instance: InstanceDetail }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const id = instance.id;
  const servers = instance.servers;
  const locked = useGameRunning(id);
  // "Katıl" stays on while only another account plays this instance (PLAY-AGAIN): this account's
  // game starts beside it. The list itself is not changed while any game of it runs.
  const playLocked = useOwnGameOf(id);
  const play = useLaunch((s) => s.play);
  const selectInstance = usePrefs((s) => s.selectInstance);
  const { layout } = useMotionPrefs();
  const [editing, setEditing] = useState<Editing | null>(null);
  const rowKeys = serverRowKeys(servers.map((server) => server.address));

  const commit = async (edit: ServerEdit) => {
    try {
      const result = await api.library.editServers(id, edit);
      queryClient.setQueryData<InstanceDetail>(["instance", id], (prev) =>
        prev ? { ...prev, servers: result.servers } : prev,
      );
      return result;
    } catch (error) {
      // The game changed the list meanwhile: show what is there now.
      if (errorCode(error) === "SUNUCU-DEGISTI") {
        void queryClient.invalidateQueries({ queryKey: ["instance", id] });
      }
      throw error;
    }
  };

  const join = (server: SavedServer) => {
    const target = parseServerAddress(server.address);
    if (!target) return;
    selectInstance(id);
    void play(id, { quickPlay: { kind: "multiplayer", host: target.host, port: target.port } });
  };

  const restore = async (index: number, entry: SavedServerEntry) => {
    try {
      await commit({ kind: "restore", index, server: entry });
      toast.success(t("instance.servers.restored", { name: entry.name }));
    } catch (error) {
      showError(error);
    }
  };

  const remove = async (index: number, server: SavedServer) => {
    try {
      const { removed } = await commit({ kind: "remove", index, was: server.address });
      toast(t("instance.servers.removed", { name: server.name }), {
        action: removed
          ? { label: t("instance.trash.undo"), onClick: () => void restore(index, removed) }
          : undefined,
      });
    } catch (error) {
      showError(error);
    }
  };

  const copyAddress = (server: SavedServer) =>
    void copyText(server.address).then(
      () => toast.success(t("instance.servers.copied")),
      () => toast.error(t("common.copyFailed")),
    );

  /** A server's actions, for its "more" menu and its right-click menu (which also offers "Katıl"). */
  const serverItems = (
    server: SavedServer,
    index: number,
    withJoin: boolean,
  ): Array<MenuItem | null> => [
    ...(withJoin
      ? [
          {
            label: t("instance.servers.join"),
            icon: <Play size={16} />,
            disabled: playLocked || !parseServerAddress(server.address),
            onSelect: () => join(server),
          },
          null,
        ]
      : []),
    {
      label: t("instance.servers.edit"),
      icon: <PencilSimple size={16} />,
      disabled: locked,
      onSelect: () => setEditing({ mode: "edit", index, server }),
    },
    {
      label: t("instance.servers.copy"),
      icon: <Copy size={16} />,
      onSelect: () => copyAddress(server),
    },
    null,
    {
      label: t("instance.servers.remove"),
      icon: <Trash size={16} />,
      danger: true,
      disabled: locked,
      onSelect: () => void remove(index, server),
    },
  ];

  const addButton = (
    <Button
      variant="secondary"
      icon={<Plus size={16} weight="bold" className="text-gold-300" />}
      disabled={locked}
      onClick={() => setEditing({ mode: "add" })}
    >
      {t("instance.servers.add")}
    </Button>
  );

  return (
    <div className="max-w-[980px] space-y-4">
      {servers.length > 0 ? (
        <div className="flex items-center gap-3">
          <p className="flex-1 text-[13px] text-ash-300 tabular-nums">
            {t("instance.servers.summary", { count: servers.length })}
          </p>
          {addButton}
        </div>
      ) : null}

      {locked ? (
        <InlineNotice tone="warn" icon={<LockSimple size={16} weight="bold" />}>
          {t("instance.servers.locked")}
        </InlineNotice>
      ) : null}

      {servers.length === 0 ? (
        <TabEmpty
          icon={<ContentIcon kind="server" />}
          title={t("instance.servers.emptyTitle")}
          action={addButton}
          className="pt-4"
        />
      ) : (
        <ul className="space-y-1">
          <AnimatePresence initial={false} mode="popLayout">
            {servers.map((server, index) => (
              <motion.li
                key={rowKeys[index]}
                layout={layout ? "position" : false}
                initial={{ opacity: 0 }}
                animate={{ opacity: 1, transition: { duration: 0.2 } }}
                exit={{ opacity: 0, transition: { duration: 0.12 } }}
                className="rounded-[var(--radius-panel)] transition-colors hover:bg-white/[0.035]"
              >
                <ContextMenu items={serverItems(server, index, true)}>
                  <div className="flex items-center gap-4 px-3 py-2.5">
                    <ServerIcon server={server} />
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-[14px] font-semibold text-bone-50">
                        {server.name}
                      </p>
                      <p className="truncate text-[12.5px] text-ash-300" data-selectable>
                        {server.address}
                      </p>
                    </div>
                    <ServerStatusLine address={server.address} />
                    <Button
                      size="sm"
                      variant="secondary"
                      icon={<Play size={14} weight="fill" className="text-gold-300" />}
                      disabled={playLocked || !parseServerAddress(server.address)}
                      onClick={() => join(server)}
                    >
                      {t("instance.servers.join")}
                    </Button>
                    <Menu
                      trigger={
                        <IconButton
                          label={t("instance.servers.more", { name: server.name })}
                          icon={<DotsThreeVertical size={18} weight="bold" />}
                          size="sm"
                        />
                      }
                      items={serverItems(server, index, false)}
                    />
                  </div>
                </ContextMenu>
              </motion.li>
            ))}
          </AnimatePresence>
        </ul>
      )}

      <ServerDialog editing={editing} onClose={() => setEditing(null)} onSave={commit} />
    </div>
  );
}

/** "Sunucu ekle" and "Düzenle": the address (required) and a name (the address when empty). */
function ServerDialog({
  editing,
  onClose,
  onSave,
}: {
  editing: Editing | null;
  onClose: () => void;
  onSave: (edit: ServerEdit) => Promise<unknown>;
}) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [address, setAddress] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!editing) return;
    setName(editing.mode === "edit" ? editing.server.name : "");
    setAddress(editing.mode === "edit" ? editing.server.address : "");
    setError(null);
    setBusy(false);
  }, [editing]);

  const submit = async (event?: FormEvent) => {
    event?.preventDefault();
    if (!editing || busy) return;
    if (!parseServerAddress(address)) {
      setError(t("instance.servers.badAddress"));
      return;
    }
    setBusy(true);
    try {
      await onSave(
        editing.mode === "add"
          ? { kind: "add", name, address }
          : {
              kind: "update",
              index: editing.index,
              was: editing.server.address,
              name,
              address,
            },
      );
      onClose();
    } catch (failure) {
      setBusy(false);
      // The backend's sentence (an address already listed, a list the game changed).
      setError(
        failure instanceof Error && failure.message && errorCode(failure)
          ? failure.message
          : t("instance.servers.saveFailed"),
      );
    }
  };

  const adding = editing?.mode !== "edit";
  return (
    <Dialog
      open={editing !== null}
      onOpenChange={(open) => !open && onClose()}
      title={adding ? t("instance.servers.addTitle") : t("instance.servers.editTitle")}
      dismissOnOutsideClick={false}
      closeLabel={t("common.close")}
      width={440}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("instance.cancel")}
          </Button>
          <Button
            variant="primary"
            disabled={busy || !address.trim()}
            onClick={() => void submit()}
          >
            {adding ? t("instance.servers.addConfirm") : t("instance.servers.save")}
          </Button>
        </>
      }
    >
      <form onSubmit={(e) => void submit(e)} className="space-y-4 pt-1">
        <Field label={t("instance.servers.address")} htmlFor="server-address" error={error}>
          <Input
            id="server-address"
            value={address}
            onChange={(e) => {
              setAddress(e.target.value);
              setError(null);
            }}
            invalid={Boolean(error)}
            placeholder={t("instance.servers.addressPlaceholder")}
            spellCheck={false}
            autoComplete="off"
            autoFocus
            maxLength={255}
          />
        </Field>
        <Field label={t("instance.servers.name")} htmlFor="server-name">
          <Input
            id="server-name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={address.trim() || t("instance.servers.namePlaceholder")}
            autoComplete="off"
            maxLength={64}
          />
        </Field>
        {/* Enter in either field saves. */}
        <button type="submit" hidden aria-hidden tabIndex={-1} />
      </form>
    </Dialog>
  );
}
