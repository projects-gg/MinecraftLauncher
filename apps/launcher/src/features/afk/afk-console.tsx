import { PaperPlaneRight, X } from "@phosphor-icons/react";
import { cn, IconButton, Input, Switch } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { Dialog } from "radix-ui";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { type AfkAccount, api } from "@/lib/api";

/** Live console for one AFK account in a side sheet. Closing it never stops the session. */
export function AfkConsole({ account, onClose }: { account: AfkAccount | undefined; onClose: () => void }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [line, setLine] = useState("");
  const [showScript, setShowScript] = useState(false);
  const listRef = useRef<HTMLDivElement>(null);
  const open = Boolean(account);

  const { data: log = [] } = useQuery({
    queryKey: ["afk-log", account?.id],
    queryFn: () => api.afkLog(account?.id ?? ""),
    enabled: open,
  });

  useEffect(
    () => api.onAfkChange(() => void queryClient.invalidateQueries({ queryKey: ["afk-log", account?.id] })),
    [queryClient, account?.id],
  );

  // Stick to the bottom as new lines arrive.
  useEffect(() => {
    const el = listRef.current;
    if (el && log.length) el.scrollTop = el.scrollHeight;
  }, [log.length]);

  const visible = showScript ? log : log.filter((l) => l.kind !== "script");

  const send = () => {
    const text = line.trim();
    if (!text || !account) return;
    void api.afkSend(account.id, text);
    setLine("");
  };

  return (
    <Dialog.Root open={open} onOpenChange={(next) => !next && onClose()}>
      <AnimatePresence>
        {open && account ? (
          <Dialog.Portal forceMount>
            <Dialog.Overlay asChild forceMount>
              <motion.div className="fixed inset-0 z-40 bg-obsidian-950/50" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} />
            </Dialog.Overlay>
            <Dialog.Content asChild forceMount aria-describedby={undefined}>
              <motion.aside
                className="glass-strong fixed top-0 right-0 bottom-0 z-50 flex w-[min(560px,92vw)] flex-col shadow-[-30px_0_80px_rgb(34_6_6/0.5)] outline-none"
                initial={{ x: "100%" }}
                animate={{ x: 0 }}
                exit={{ x: "100%", transition: { duration: 0.22 } }}
                transition={{ duration: 0.4, ease: [0.16, 1, 0.3, 1] }}
              >
                <header className="flex items-center justify-between border-b border-white/8 px-5 py-4">
                  <Dialog.Title className="font-display text-[18px] font-semibold text-bone-50">
                    {t("afk.consoleTitle", { name: account.nickname })}
                  </Dialog.Title>
                  <div className="flex items-center gap-3">
                    <Switch checked={showScript} onCheckedChange={setShowScript} label={t("afk.showScript")} />
                    <Dialog.Close asChild>
                      <IconButton label={t("common.close")} icon={<X size={18} />} size="sm" />
                    </Dialog.Close>
                  </div>
                </header>
                <div ref={listRef} className="min-h-0 flex-1 overflow-y-auto px-5 py-3 font-mono text-[12.5px] leading-relaxed" data-selectable role="log" aria-live="polite">
                  {visible.map((l) => (
                    <p
                      key={l.id}
                      className={cn(
                        "break-words",
                        l.kind === "error" && "text-flare-400",
                        l.kind === "system" && "text-ash-400",
                        l.kind === "script" && "text-gold-300/80",
                        l.kind === "self" && "text-gold-200",
                        l.kind === "chat" && "text-bone-100",
                      )}
                    >
                      <span className="mr-2 text-ash-500">
                        {new Date(l.time).toLocaleTimeString("tr-TR", { hour: "2-digit", minute: "2-digit", second: "2-digit" })}
                      </span>
                      {l.text}
                    </p>
                  ))}
                </div>
                <form
                  className="flex gap-2 border-t border-white/8 p-4"
                  onSubmit={(e) => {
                    e.preventDefault();
                    send();
                  }}
                >
                  <Input
                    aria-label={t("afk.consoleInput")}
                    placeholder={t("afk.consoleInput")}
                    value={line}
                    onChange={(e) => setLine(e.target.value)}
                    spellCheck={false}
                    autoComplete="off"
                    className="font-mono text-[13px]"
                  />
                  <IconButton label={t("afk.send")} icon={<PaperPlaneRight size={18} weight="fill" />} type="submit" className="bg-gold-500 text-gold-ink hover:bg-gold-400 hover:text-gold-ink" />
                </form>
              </motion.aside>
            </Dialog.Content>
          </Dialog.Portal>
        ) : null}
      </AnimatePresence>
    </Dialog.Root>
  );
}
