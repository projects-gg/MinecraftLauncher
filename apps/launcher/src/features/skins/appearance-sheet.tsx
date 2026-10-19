// "Görünüm": skin and cape of a Microsoft account, opened from Settings → Hesaplar.
import {
  ArrowCounterClockwise,
  ArrowsClockwise,
  CloudSlash,
  Copy,
  FolderOpen,
  Prohibit,
  UploadSimple,
  WarningCircle,
  X,
} from "@phosphor-icons/react";
import {
  Button,
  Chip,
  Dialog as ConfirmDialog,
  cn,
  DropZone,
  EntranceGate,
  easeOutExpo,
  GatedMotion,
  IconButton,
  PixelBrackets,
  PlayerHead,
  Segmented,
  Skeleton,
  SwapLabel,
  scrimMotion,
  sheetMotion,
  toast,
  useInteractionMotion,
  useMotionPrefs,
} from "@pl/ui";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, useAnimate } from "motion/react";
import { Dialog, RadioGroup } from "radix-ui";
import {
  lazy,
  type ReactNode,
  Suspense,
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { useDropAnnouncement, useDropZone } from "@/features/drop/store";
import { announcementOf, zoneTitle } from "@/features/drop/text";
import { isSkinFile } from "@/features/drop/zones";
import { MicrosoftMark } from "@/features/settings/microsoft-mark";
import {
  type AccountSummary,
  api,
  type SkinAppearance,
  type SkinFile,
  type SkinHistory,
  type SkinHistoryEntry,
  type SkinVariant,
} from "@/lib/api";
import { copyDetails } from "@/lib/clipboard";
import { capeFront, inferVariant, loadImage } from "./skin-art";
import { SkinFlat } from "./skin-flat";
import { historyKey, SkinHistorySection, useSkinHistory } from "./skin-history";
import type { SkinTurn } from "./skin-viewer";

// three.js is large: the 3D preview loads the first time the panel opens.
const SkinViewer3D = lazy(() => import("./skin-viewer"));

/** Radio value of "no cape". */
const NO_CAPE = "__none__";

/** A failed command carries a friendly message plus details for "copy details". */
interface FriendlyError {
  message: string;
  details: string | null;
  code: string;
}

function friendly(error: unknown, fallback: string): FriendlyError {
  if (error instanceof Error && "details" in error && typeof error.details === "string") {
    const code = "code" in error && typeof error.code === "string" ? error.code : "";
    return { message: error.message, details: `${code}\n${error.details}`.trim(), code };
  }
  // Anything else (IPC failure, programming error) is not meant for the player's eyes.
  return {
    message: fallback,
    details: error instanceof Error ? error.message : String(error),
    code: "",
  };
}

/** Sessions that only a new sign-in can fix (expired or revoked refresh token, lost secrets). */
const needsSignIn = (code: string) => code.startsWith("MSA-") || code === "KC-YOK";

/** Minecraft refused a skin change as too frequent; "Giy" rests this long afterwards. */
const RATE_LIMITED_MS = 60_000;
const isRateLimited = (code: string) => code.endsWith("-429");

/**
 * Failures of putting a history skin back on that say nothing useful to the player as they are
 * (Minecraft's refusal texts speak of picking a file): a short message of our own instead.
 */
const genericWearFailure = (code: string) =>
  code === "" || code.startsWith("MC-SKIN-") || code.startsWith("VERI-");

/** A texture as a data URL the WebGL preview can use. URLs are content-addressed: cache long. */
function useTexture(url: string | undefined) {
  return useQuery({
    queryKey: ["skin-texture", url],
    queryFn: () => api.skins.texture(url ?? ""),
    enabled: Boolean(url),
    staleTime: Number.POSITIVE_INFINITY,
    gcTime: 30 * 60_000,
    retry: 1,
  });
}

/** Height of a texture is half its width for legacy 64×32 skins. */
function useLegacyTexture(src: string | null) {
  const [legacy, setLegacy] = useState(false);
  useEffect(() => {
    if (!src) return;
    let live = true;
    loadImage(src)
      .then((image) => {
        if (live) setLegacy(image.naturalHeight * 2 === image.naturalWidth);
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [src]);
  return legacy;
}

/** Placeholder shaped like the player (head, body and arms, legs). */
function FigureSkeleton() {
  return (
    <div aria-hidden className="absolute inset-0 flex flex-col items-center justify-center gap-1">
      <Skeleton className="size-14" />
      <div className="flex gap-1">
        <Skeleton className="h-[84px] w-6" />
        <Skeleton className="h-[84px] w-14" />
        <Skeleton className="h-[84px] w-6" />
      </div>
      <div className="flex gap-1">
        <Skeleton className="h-[84px] w-[26px]" />
        <Skeleton className="h-[84px] w-[26px]" />
      </div>
    </div>
  );
}

function SectionHead({ id, title, meta }: { id: string; title: string; meta?: ReactNode }) {
  return (
    <div className="mb-3 flex items-baseline justify-between gap-4">
      <h3 id={id} className="font-display text-[17px] leading-tight font-semibold text-bone-50">
        {title}
      </h3>
      {meta ? <span className="shrink-0 text-[12.5px] text-ash-400">{meta}</span> : null}
    </div>
  );
}

function CapeTile({
  value,
  label,
  art,
  selected,
}: {
  value: string;
  label: string;
  art?: string;
  selected: boolean;
}) {
  return (
    <RadioGroup.Item
      value={value}
      className={cn(
        "relative flex cursor-pointer flex-col items-center gap-2 rounded-[var(--radius-panel)] px-2 pt-2.5 pb-2 transition-colors",
        "focus-visible:ring-2 focus-visible:ring-gold-300 focus-visible:outline-none disabled:cursor-default",
        selected
          ? "bg-gold-500/12 ring-1 ring-gold-400/50"
          : "bg-obsidian-950/35 ring-1 ring-white/7 hover:bg-white/5 hover:ring-white/15",
      )}
    >
      {selected ? <PixelBrackets size={8} /> : null}
      <span
        aria-hidden
        className="flex h-[80px] w-[50px] items-center justify-center rounded-[4px] bg-obsidian-950/70"
        style={art ? capeFront(art) : undefined}
      >
        {art ? null : <Prohibit size={22} className="text-ash-400" />}
      </span>
      <span
        className={cn(
          "line-clamp-2 min-h-[32px] w-full text-center text-[12px] leading-[16px] font-medium break-words",
          selected ? "text-gold-200" : "text-bone-100",
        )}
      >
        {label}
      </span>
    </RadioGroup.Item>
  );
}

/** A skin of the history being put back on: shown at once, confirmed by Minecraft later. */
interface Wearing {
  entry: SkinHistoryEntry;
  /** Its texture, once loaded (the big preview shows it from then on). */
  src: string | null;
}

function Editor({
  account,
  view,
  history,
  onUpdate,
  onDirtyChange,
  onRetry,
  onSignIn,
  refreshing,
}: {
  account: AccountSummary;
  view: SkinAppearance;
  history: SkinHistory | null;
  onUpdate: (view: SkinAppearance) => void;
  onDirtyChange: (dirty: boolean) => void;
  onRetry: () => void;
  onSignIn: () => void;
  refreshing: boolean;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const ids = useId();
  const [file, setFile] = useState<SkinFile | null>(null);
  const [variantDraft, setVariantDraft] = useState<SkinVariant | null>(null);
  const [capeDraft, setCapeDraft] = useState<{ id: string | null } | null>(null);
  const [guessed, setGuessed] = useState(false);
  const [fileError, setFileError] = useState<string | null>(null);
  const [reading, setReading] = useState(false);
  const [turn, setTurn] = useState<SkinTurn | null>(null);
  const [flat, setFlat] = useState(false);
  const [confirmReset, setConfirmReset] = useState(false);
  const [saveError, setSaveError] = useState<FriendlyError | null>(null);
  const [wearing, setWearing] = useState<Wearing | null>(null);
  const [restingUntil, setRestingUntil] = useState(0);

  const baseVariant = view.skin?.variant ?? "classic";
  const baseCape = view.capes.find((c) => c.active)?.id ?? null;
  const variant = variantDraft ?? (wearing?.src ? wearing.entry.variant : baseVariant);
  const capeId = capeDraft ? capeDraft.id : baseCape;
  const variantChanged = variantDraft !== null && variantDraft !== baseVariant;
  const capeChanged = capeId !== baseCape;
  const dirty = file !== null || variantChanged || capeChanged;
  useEffect(() => onDirtyChange(dirty), [dirty, onDirtyChange]);

  const skinTexture = useTexture(view.skin?.url);
  const cape = view.capes.find((c) => c.id === capeId) ?? null;
  const capeTexture = useTexture(cape?.url);
  const skinSrc = file?.dataUrl ?? wearing?.src ?? skinTexture.data ?? null;
  const capeSrc = cape ? (capeTexture.data ?? null) : null;
  const legacy = useLegacyTexture(skinSrc);
  const facing = turn?.side ?? "front";

  // "Giy" rests for a minute after Minecraft refused a change as too frequent.
  const resting = restingUntil > 0;
  useEffect(() => {
    if (!restingUntil) return;
    const timer = window.setTimeout(() => setRestingUntil(0), restingUntil - Date.now());
    return () => window.clearTimeout(timer);
  }, [restingUntil]);

  // The big preview changes skins with a quick dip: the figure fades (and settles a little
  // toward the floor), the new skin goes on, and it comes back once it is drawn. A newer change
  // takes over from wherever the last one is. Reduced motion and Performance mode keep the fade
  // without the movement.
  const [figureScope, animateFigure] = useAnimate<HTMLDivElement>();
  const { layout: glide } = useMotionPrefs();
  const swapToken = useRef(0);
  const skinShown = useRef<(() => void) | null>(null);
  const onSkinShown = useCallback(() => {
    skinShown.current?.();
    skinShown.current = null;
  }, []);
  const swapFigure = useCallback(
    async (apply: () => void) => {
      const element = figureScope.current;
      if (!element) {
        apply();
        return;
      }
      const token = ++swapToken.current;
      // `scale` (not a transform string): it starts from 1 the first time, where a string
      // would start from no transform at all and grow the figure out of nothing.
      await animateFigure(element, glide ? { opacity: 0, scale: 0.96 } : { opacity: 0 }, {
        duration: 0.1,
        ease: easeOutExpo,
      });
      if (token !== swapToken.current) return;
      await new Promise<void>((resolve) => {
        skinShown.current = resolve;
        // A skin that does not change (or the flat view) reports nothing: go on shortly.
        window.setTimeout(resolve, flat ? 32 : 200);
        apply();
      });
      if (token !== swapToken.current) return;
      void animateFigure(element, glide ? { opacity: 1, scale: 1 } : { opacity: 1 }, {
        duration: 0.3,
        ease: easeOutExpo,
      });
    },
    [animateFigure, figureScope, flat, glide],
  );

  // Cape textures are tiny: fetch them all up front so a picked cape shows at once.
  useEffect(() => {
    for (const c of view.capes) {
      void queryClient.prefetchQuery({
        queryKey: ["skin-texture", c.url],
        queryFn: () => api.skins.texture(c.url),
        staleTime: Number.POSITIVE_INFINITY,
      });
    }
  }, [queryClient, view.capes]);

  const clearFile = () => {
    setFile(null);
    setVariantDraft(null);
    setGuessed(false);
  };

  const accountsChanged = () => void queryClient.invalidateQueries({ queryKey: ["accounts"] });
  const historyChanged = () =>
    void queryClient.invalidateQueries({ queryKey: historyKey(account.id) });
  /** The history's idea of the skin worn now, ahead of its next read. */
  const markCurrent = (id: string | null) =>
    queryClient.setQueryData<SkinHistory>(historyKey(account.id), (old) =>
      old ? { ...old, currentId: id } : old,
    );

  const wearingRef = useRef(wearing);
  wearingRef.current = wearing;
  const wear = useMutation({
    mutationFn: (entry: SkinHistoryEntry) => api.skins.wearFromHistory(account.id, entry.id),
    onSuccess: (next, entry) => {
      // The preview shows this texture already: keep it for the new skin's address.
      const src = wearingRef.current?.src;
      if (next.skin && src && !queryClient.getQueryData(["skin-texture", next.skin.url]))
        queryClient.setQueryData(["skin-texture", next.skin.url], src);
      markCurrent(entry.id);
      onUpdate(next);
      setWearing(null);
      accountsChanged();
      historyChanged();
      toast.success(t("skins.history.worn"));
    },
    onError: (error) => {
      // Back to the skin worn before, with the same dip if the preview had changed.
      if (wearingRef.current?.src) void swapFigure(() => setWearing(null));
      else setWearing(null);
      const { message, details, code } = friendly(error, t("skins.history.wearFailed"));
      if (isRateLimited(code)) {
        setRestingUntil(Date.now() + RATE_LIMITED_MS);
        toast.error(t("skins.history.rateLimited"));
      } else if (needsSignIn(code)) {
        toast.error(message, { action: { label: t("skins.signInAgain"), onClick: onSignIn } });
      } else {
        toast.error(genericWearFailure(code) ? t("skins.history.wearFailed") : message, {
          action: details
            ? { label: t("common.copyDetails"), onClick: () => void copyDetails(details) }
            : undefined,
        });
      }
    },
  });

  // What `startWear` checks, read when it runs: toast actions keep the version of the render
  // that made them.
  const pendingWear = useRef<string | null>(null);
  const blocked = useRef({ busy: false, resting: false });

  /** Puts a skin of the history back on: the preview changes at once, Minecraft confirms. */
  const startWear = (entry: SkinHistoryEntry) => {
    if (pendingWear.current || blocked.current.busy || blocked.current.resting) return;
    pendingWear.current = entry.id;
    // The worn skin replaces a file or arm model that was only being tried.
    clearFile();
    setFileError(null);
    setSaveError(null);
    setWearing({ entry, src: null });
    void queryClient
      .fetchQuery({
        queryKey: ["skin-texture", entry.url],
        queryFn: () => api.skins.texture(entry.url),
        staleTime: Number.POSITIVE_INFINITY,
      })
      .then(async (src) => {
        // Decoded before the dip, so the figure is gone only for as long as drawing it takes.
        await loadImage(src).catch(() => null);
        if (pendingWear.current !== entry.id) return;
        void swapFigure(() => {
          setWearing((current) => (current?.entry.id === entry.id ? { ...current, src } : current));
          setTurn({ side: "front", key: Date.now() });
        });
      })
      .catch(() => {});
    wear.mutate(entry, {
      onSettled: () => {
        pendingWear.current = null;
      },
    });
  };
  // The grid gets one function for good, which always runs the latest `startWear`.
  const startWearRef = useRef(startWear);
  startWearRef.current = startWear;
  const wearFromGrid = useCallback((entry: SkinHistoryEntry) => startWearRef.current(entry), []);

  const mounted = useRef(true);
  useEffect(
    () => () => {
      mounted.current = false;
    },
    [],
  );

  /** "Geri al" after going back to the default skin: the skin worn before, from the history. */
  const undoReset = (previousId: string) => {
    const entry = queryClient
      .getQueryData<SkinHistory>(historyKey(account.id))
      ?.entries.find((other) => other.id === previousId);
    if (!entry) return;
    if (mounted.current) {
      startWearRef.current(entry);
      return;
    }
    // The panel was closed meanwhile: put it back on all the same.
    api.skins
      .wearFromHistory(account.id, entry.id)
      .then(() => {
        accountsChanged();
        toast.success(t("skins.history.worn"));
      })
      .catch((error) => toast.error(friendly(error, t("skins.history.wearFailed")).message));
  };

  const save = useMutation({
    mutationFn: async () => {
      if (file) {
        const next = await api.skins.upload(account.id, file.path, variant);
        // The new texture is the file just shown: no need to download it again.
        if (next.skin) queryClient.setQueryData(["skin-texture", next.skin.url], file.dataUrl);
        onUpdate(next);
        clearFile();
      } else if (variantChanged) {
        const texture = skinTexture.data;
        const next = await api.skins.setVariant(account.id, variant);
        if (next.skin && texture && !queryClient.getQueryData(["skin-texture", next.skin.url]))
          queryClient.setQueryData(["skin-texture", next.skin.url], texture);
        onUpdate(next);
        setVariantDraft(null);
      }
      if (capeChanged) {
        onUpdate(await api.skins.setCape(account.id, capeId));
        setCapeDraft(null);
      }
    },
    onSuccess: () => {
      setSaveError(null);
      accountsChanged();
      historyChanged();
      toast.success(t("skins.saved"));
    },
    onError: (error) => {
      // Part of the change may have gone through (skin saved, cape not).
      accountsChanged();
      historyChanged();
      setSaveError(friendly(error, t("skins.saveFailed")));
    },
  });

  // The skin worn before stays in the history, so going back to the default skin needs no
  // question: "Geri al" in the toast puts it back on. Without it (the history could not keep
  // it) the panel asks first, as the old skin would be gone.
  const resetUndoable = Boolean(history?.currentId && view.skin && !view.skin.isDefault);
  const reset = useMutation({
    mutationFn: (_previousId: string | null) => api.skins.reset(account.id),
    onSuccess: async (next, previousId) => {
      clearFile();
      setFileError(null);
      setSaveError(null);
      // The default skin's picture first, so the figure changes in one dip instead of
      // disappearing while it loads.
      const url = next.skin?.url;
      if (url)
        await queryClient
          .fetchQuery({
            queryKey: ["skin-texture", url],
            queryFn: () => api.skins.texture(url),
            staleTime: Number.POSITIVE_INFINITY,
          })
          .catch(() => null);
      void swapFigure(() => {
        onUpdate(next);
        setTurn({ side: "front", key: Date.now() });
      });
      markCurrent(null);
      accountsChanged();
      historyChanged();
      toast.success(
        t("skins.reset.done"),
        previousId
          ? { action: { label: t("skins.history.undo"), onClick: () => undoReset(previousId) } }
          : undefined,
      );
    },
    onError: (error) => {
      const { message, details } = friendly(error, t("skins.reset.failed"));
      toast.error(message, {
        action: details
          ? { label: t("common.copyDetails"), onClick: () => void copyDetails(details) }
          : undefined,
      });
    },
  });

  const busy = save.isPending || reset.isPending;
  /** A history skin is being put on: the other changes wait for it. */
  const putting = wearing !== null;
  blocked.current = { busy, resting };

  const intake = async (path: string) => {
    setFileError(null);
    setSaveError(null);
    setReading(true);
    try {
      const read = await api.skins.readFile(path);
      const guess = await inferVariant(read.dataUrl).catch(() => null);
      setFile(read);
      setVariantDraft(guess);
      setGuessed(guess !== null);
      setTurn({ side: "front", key: Date.now() });
    } catch (error) {
      setFileError(friendly(error, t("skins.file.readFailed")).message);
    } finally {
      setReading(false);
    }
  };

  const choose = async () => {
    try {
      const path = await api.skins.pickFile();
      if (path) await intake(path);
    } catch (error) {
      setFileError(friendly(error, t("skins.file.pickFailed")).message);
    }
  };

  // Files dropped anywhere on the window while the panel is open come here (the window-wide
  // drop layer steps aside meanwhile); the panel's body shows the zone. While the panel is busy
  // it shows none and the drop is ignored.
  const dropBlocked = busy || putting || reading || confirmReset;
  const blockedRef = useRef(dropBlocked);
  blockedRef.current = dropBlocked;
  const intakeRef = useRef(intake);
  intakeRef.current = intake;
  const drop = useDropZone({
    layer: "claim",
    accepts: (drag) => !blockedRef.current && drag.paths.some(isSkinFile),
    onDrop: (paths) => {
      if (blockedRef.current) return;
      // A PNG among the files wins; anything else is read and refused with its usual message.
      const path = paths.find(isSkinFile) ?? paths[0];
      if (path) void intakeRef.current(path);
    },
  });
  const dropTitle = zoneTitle(t, drop.state, drop.drag?.paths.length ?? 1);
  const dropHint = drop.state === "refuse" ? t("skins.file.dropRefuse") : t("skins.file.dropHint");
  const dropShown = (drop.state !== "idle" && !dropBlocked) || drop.state === "absorb";
  useDropAnnouncement(
    dropShown && drop.state !== "absorb" ? announcementOf(drop.state, dropTitle, dropHint) : null,
    drop.drag?.id,
  );

  const pickCape = (id: string | null) => {
    setCapeDraft({ id });
    setSaveError(null);
    setTurn({ side: id ? "back" : "front", key: Date.now() });
  };

  const revert = () => {
    clearFile();
    setCapeDraft(null);
    setFileError(null);
    setSaveError(null);
  };

  const skinMeta = view.skin
    ? view.skin.isDefault
      ? t("skins.skin.default")
      : t("skins.skin.custom")
    : undefined;

  const stage = (
    <div className="relative min-h-0 flex-1 overflow-hidden rounded-[var(--radius-panel)] bg-obsidian-950/45 ring-1 ring-white/6">
      <div
        aria-hidden
        className="pointer-events-none absolute inset-x-8 bottom-7 h-14 rounded-[50%] bg-[radial-gradient(closest-side,rgb(232_154_44/0.24),transparent)]"
      />
      <div
        ref={figureScope}
        role="img"
        aria-label={t("skins.preview.label")}
        // The dip on a skin change settles toward the floor glow.
        className="absolute inset-0 origin-[50%_82%]"
      >
        {!view.skin && !file ? (
          <p className="absolute inset-0 flex items-center justify-center px-8 text-center text-[13px] text-ash-400">
            {t("skins.preview.noSkin")}
          </p>
        ) : flat ? (
          skinSrc ? (
            // Feet on the floor glow, like the 3D model.
            <div className="absolute inset-0 flex items-end justify-center pb-[52px]">
              <SkinFlat src={skinSrc} variant={variant} legacy={legacy} scale={8} />
            </div>
          ) : null
        ) : (
          <Suspense fallback={skinSrc ? <FigureSkeleton /> : null}>
            <SkinViewer3D
              className="absolute inset-0"
              skin={skinSrc}
              variant={variant}
              cape={capeSrc}
              turn={turn}
              onUnsupported={() => setFlat(true)}
              onSkinShown={onSkinShown}
            />
          </Suspense>
        )}
        {(view.skin || file) && !skinSrc && !skinTexture.isError ? <FigureSkeleton /> : null}
      </div>
      {skinTexture.isError && !file && !wearing?.src ? (
        <div className="absolute inset-x-4 bottom-4 flex flex-col items-center gap-2.5 text-center">
          <p className="text-[13px] text-ash-300">{t("skins.preview.failed")}</p>
          <Button size="sm" variant="secondary" onClick={() => void skinTexture.refetch()}>
            {t("common.retry")}
          </Button>
        </div>
      ) : null}
      {dirty ? (
        <Chip tone="gold" className="absolute top-3 left-3">
          {t("skins.preview.unsaved")}
        </Chip>
      ) : null}
      {flat ? null : (
        <IconButton
          className="absolute top-2 right-2"
          size="sm"
          tooltipSide="left"
          label={facing === "back" ? t("skins.preview.showFront") : t("skins.preview.showBack")}
          icon={<ArrowsClockwise size={17} />}
          onClick={() => setTurn({ side: facing === "back" ? "front" : "back", key: Date.now() })}
        />
      )}
    </div>
  );

  return (
    <>
      <div ref={drop.ref} className="relative flex min-h-0 flex-1">
        <div className="flex w-[320px] shrink-0 flex-col border-r border-white/7 p-5">
          {stage}
          <p className="mt-3 text-center text-[12.5px] text-ash-400">
            {flat ? t("skins.preview.flat") : t("skins.preview.hint")}
          </p>
        </div>

        <fieldset
          disabled={busy}
          className="m-0 min-h-0 min-w-0 flex-1 overflow-y-auto border-0 px-7 pt-6 pb-8"
        >
          {view.stale ? (
            <div className="mb-6 flex items-center gap-3 rounded-[var(--radius-panel)] bg-gold-500/10 px-4 py-3 ring-1 ring-gold-400/25">
              <CloudSlash size={18} className="shrink-0 text-gold-300" />
              <p className="min-w-0 flex-1 text-[13px] text-gold-200">{t("skins.stale")}</p>
              <Button
                size="sm"
                variant="secondary"
                icon={<ArrowsClockwise size={14} />}
                disabled={refreshing}
                onClick={onRetry}
              >
                {t("common.retry")}
              </Button>
            </div>
          ) : null}

          <section aria-labelledby={`${ids}-skin`}>
            <SectionHead id={`${ids}-skin`} title={t("skins.skin.title")} meta={skinMeta} />
            {file ? (
              <div className="flex items-center gap-4 rounded-[var(--radius-panel)] bg-obsidian-950/35 py-3 pr-3 pl-3 ring-1 ring-gold-400/30">
                <div className="flex h-[108px] w-[68px] shrink-0 items-center justify-center rounded-md bg-obsidian-950/60">
                  <SkinFlat
                    src={file.dataUrl}
                    variant={variant}
                    legacy={file.height === 32}
                    scale={3}
                  />
                </div>
                <div className="min-w-0 flex-1">
                  <p className="truncate text-[14px] font-medium text-bone-50" data-selectable>
                    {file.name}
                  </p>
                  <p className="mt-0.5 text-[12.5px] text-ash-400 tabular-nums">
                    {t("skins.file.size", { width: file.width, height: file.height })}
                  </p>
                  <p className="mt-1.5 text-[12.5px] text-gold-200">{t("skins.file.pending")}</p>
                </div>
                <Button size="sm" variant="ghost" onClick={() => void choose()} disabled={reading}>
                  {t("skins.file.another")}
                </Button>
                <IconButton
                  size="sm"
                  label={t("skins.file.remove")}
                  icon={<X size={16} />}
                  onClick={() => {
                    clearFile();
                    setFileError(null);
                  }}
                />
              </div>
            ) : (
              <div className="flex items-center gap-4 rounded-[var(--radius-panel)] border border-dashed border-white/14 bg-obsidian-950/30 px-5 py-4">
                <UploadSimple size={26} className="shrink-0 text-ash-300" />
                <div className="min-w-0 flex-1">
                  <p className="text-[14px] font-medium text-bone-50">{t("skins.file.title")}</p>
                  <p className="mt-0.5 text-[12.5px] text-ash-400">{t("skins.file.body")}</p>
                  <p className="mt-0.5 text-[12.5px] text-ash-400">{t("skins.file.help")}</p>
                </div>
                <Button
                  size="sm"
                  variant="secondary"
                  icon={<FolderOpen size={15} />}
                  onClick={() => void choose()}
                  disabled={reading || putting}
                >
                  {reading ? t("skins.file.reading") : t("skins.file.choose")}
                </Button>
              </div>
            )}
            {fileError ? (
              <p
                role="alert"
                className="mt-2.5 flex items-start gap-2 text-[12.5px] text-flare-400"
              >
                <WarningCircle size={15} weight="fill" className="mt-px shrink-0" />
                <span>{fileError}</span>
              </p>
            ) : null}

            <div className="mt-5 flex items-center justify-between gap-6">
              <div className="min-w-0">
                <p className="text-[13.5px] font-medium text-bone-50">{t("skins.model.title")}</p>
                {/* The 3D preview shows the arm width; only a guess from the file is worth a word. */}
                {guessed && file ? (
                  <p className="mt-0.5 max-w-[46ch] text-[12.5px] text-ash-400">
                    {t("skins.model.guessed")}
                  </p>
                ) : null}
              </div>
              <Segmented<SkinVariant>
                value={variant}
                label={t("skins.model.title")}
                onValueChange={(next) => {
                  setVariantDraft(next);
                  setGuessed(false);
                  setSaveError(null);
                }}
                options={[
                  { value: "classic", label: t("skins.model.classic") },
                  { value: "slim", label: t("skins.model.slim") },
                ]}
              />
            </div>
          </section>

          {history ? (
            <SkinHistorySection
              accountId={account.id}
              history={history}
              currentId={wearing?.entry.id ?? history.currentId}
              pendingId={wearing?.entry.id ?? null}
              wearBlock={
                view.stale ? "offline" : resting ? "cooldown" : busy || putting ? "busy" : null
              }
              onWear={wearFromGrid}
            />
          ) : null}

          <section aria-labelledby={`${ids}-cape`} className="mt-8">
            <SectionHead
              id={`${ids}-cape`}
              title={t("skins.cape.title")}
              meta={
                view.capes.length > 0 ? t("skins.cape.count", { count: view.capes.length }) : null
              }
            />
            {view.capes.length === 0 ? (
              <p className="rounded-[var(--radius-panel)] bg-obsidian-950/30 px-4 py-3.5 text-[13px] text-ash-400 ring-1 ring-white/6">
                {t("skins.cape.empty")}
              </p>
            ) : (
              <RadioGroup.Root
                value={capeId ?? NO_CAPE}
                onValueChange={(value) => pickCape(value === NO_CAPE ? null : value)}
                disabled={putting}
                aria-labelledby={`${ids}-cape`}
                className="grid grid-cols-[repeat(auto-fill,minmax(88px,1fr))] gap-3 p-1"
              >
                <CapeTile value={NO_CAPE} label={t("skins.cape.none")} selected={capeId === null} />
                {view.capes.map((c) => (
                  <CapeTile
                    key={c.id}
                    value={c.id}
                    label={c.name}
                    art={c.url}
                    selected={capeId === c.id}
                  />
                ))}
              </RadioGroup.Root>
            )}
          </section>

          <section className="mt-8 flex items-center justify-between gap-6 border-t border-white/7 pt-5">
            <div className="min-w-0">
              <p className="text-[13.5px] font-medium text-bone-50">{t("skins.reset.title")}</p>
              {view.skin?.isDefault ? (
                <p className="mt-0.5 max-w-[50ch] text-[12.5px] text-ash-400">
                  {t("skins.reset.already")}
                </p>
              ) : null}
            </div>
            <Button
              size="sm"
              variant="secondary"
              icon={<ArrowCounterClockwise size={15} />}
              disabled={Boolean(view.skin?.isDefault) || !view.skin || putting}
              onClick={() =>
                resetUndoable ? reset.mutate(history?.currentId ?? null) : setConfirmReset(true)
              }
            >
              {t("skins.reset.action")}
            </Button>
          </section>
        </fieldset>

        <AnimatePresence>
          {dropShown ? (
            <EntranceGate key="drop">
              <DropZone
                aria-hidden
                variant="cover"
                state={drop.state}
                title={dropTitle}
                hint={dropHint}
                className="pointer-events-none absolute inset-3 z-10"
              />
            </EntranceGate>
          ) : null}
        </AnimatePresence>
      </div>

      <footer className="flex items-center gap-3 border-t border-white/8 bg-obsidian-950/40 px-5 py-3.5">
        <div className="min-w-0 flex-1 text-[12.5px]" aria-live="polite">
          {saveError ? (
            <div className="flex items-center gap-2">
              <p role="alert" className="min-w-0 text-flare-400">
                {saveError.message}
              </p>
              {saveError.details ? (
                <Button
                  size="sm"
                  variant="ghost"
                  icon={<Copy size={14} />}
                  onClick={() => void copyDetails(saveError.details ?? "")}
                >
                  {t("common.copyDetails")}
                </Button>
              ) : null}
            </div>
          ) : save.isPending || putting ? (
            <p className="text-ash-300">{t("skins.savingHint")}</p>
          ) : dirty ? (
            <p className="text-gold-200">{t("skins.dirty")}</p>
          ) : (
            <p className="text-ash-400">{t("skins.hint")}</p>
          )}
        </div>
        <Button variant="ghost" onClick={revert} disabled={!dirty || busy || putting}>
          {t("skins.revert")}
        </Button>
        <Button
          variant="primary"
          onClick={() => save.mutate()}
          disabled={!dirty || busy || putting}
        >
          <SwapLabel
            labels={[t("skins.save"), t("skins.saving")]}
            active={save.isPending ? 1 : 0}
          />
        </Button>
      </footer>

      <ConfirmDialog
        open={confirmReset}
        onOpenChange={setConfirmReset}
        title={t("skins.reset.confirmTitle")}
        description={t("skins.reset.confirmBody")}
        closeLabel={t("common.close")}
        width={440}
        footer={
          <>
            <Button variant="ghost" onClick={() => setConfirmReset(false)}>
              {t("skins.reset.cancel")}
            </Button>
            <Button
              variant="danger"
              onClick={() => {
                setConfirmReset(false);
                reset.mutate(history?.currentId ?? null);
              }}
            >
              {t("skins.reset.confirm")}
            </Button>
          </>
        }
      />
    </>
  );
}

function SheetSkeleton() {
  return (
    <div className="flex min-h-0 flex-1">
      <div className="flex w-[320px] shrink-0 flex-col border-r border-white/7 p-5">
        <div className="relative min-h-0 flex-1 rounded-[var(--radius-panel)] bg-obsidian-950/45 ring-1 ring-white/6">
          <FigureSkeleton />
        </div>
        <Skeleton className="mx-auto mt-3 h-4 w-36" />
      </div>
      <div className="flex-1 space-y-4 px-7 pt-6">
        <Skeleton className="h-5 w-24" />
        <Skeleton className="h-[86px] w-full" />
        <Skeleton className="h-10 w-full" />
        <Skeleton className="mt-8 h-5 w-24" />
        <div className="flex gap-3">
          {["a", "b", "c", "d"].map((k) => (
            <Skeleton key={k} className="h-[124px] w-[88px]" />
          ))}
        </div>
      </div>
    </div>
  );
}

function LoadError({
  error,
  retrying,
  onRetry,
  onSignIn,
}: {
  error: unknown;
  retrying: boolean;
  onRetry: () => void;
  onSignIn: () => void;
}) {
  const { t } = useTranslation();
  const { message, details, code } = friendly(error, t("skins.loadFailedBody"));
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-2 p-10 text-center">
      <p className="font-display text-lg font-semibold text-bone-50">{t("skins.loadFailed")}</p>
      <p role="alert" className="max-w-[48ch] text-ash-400">
        {message}
      </p>
      <div className="mt-4 flex flex-wrap items-center justify-center gap-2.5">
        {needsSignIn(code) ? (
          <Button variant="primary" icon={<MicrosoftMark size={15} />} onClick={onSignIn}>
            {t("skins.signInAgain")}
          </Button>
        ) : null}
        <Button variant="secondary" onClick={onRetry} disabled={retrying}>
          {t("common.retry")}
        </Button>
        {details ? (
          <Button
            variant="ghost"
            icon={<Copy size={14} />}
            onClick={() => void copyDetails(details)}
          >
            {t("common.copyDetails")}
          </Button>
        ) : null}
      </div>
    </div>
  );
}

/** The side sheet of a Microsoft account. Closing with unsaved changes asks first. */
function AppearanceSheetPanel({
  account,
  onClose,
  onSignIn,
}: {
  account: AccountSummary | null;
  onClose: () => void;
  onSignIn: () => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const animate = useInteractionMotion();
  const open = Boolean(account);
  // Both belong to one account: switching accounts never inherits them.
  const dirtyRef = useRef<{ id: string; dirty: boolean } | null>(null);
  const [confirmFor, setConfirmFor] = useState<string | null>(null);
  const confirmOpen = open && confirmFor === account?.id;
  const setConfirmOpen = (next: boolean) => setConfirmFor(next ? (account?.id ?? null) : null);
  const queryKey = ["skin-appearance", account?.id];
  const {
    data: view,
    error,
    isError,
    refetch,
    isFetching,
  } = useQuery({
    queryKey,
    queryFn: () => api.skins.appearance(account?.id ?? ""),
    enabled: open,
    staleTime: 0,
    gcTime: 0,
    retry: false,
    refetchOnWindowFocus: false,
  });
  // Read after the profile: reading it records the skin worn now in the history.
  const history = useSkinHistory(account?.id ?? "", open && Boolean(view));
  const retry = () =>
    void refetch().then(() => {
      if (account) void queryClient.invalidateQueries({ queryKey: historyKey(account.id) });
    });

  const requestClose = () => {
    const dirty = dirtyRef.current;
    if (account && dirty?.id === account.id && dirty.dirty) setConfirmOpen(true);
    else onClose();
  };

  return (
    <Dialog.Root open={open} onOpenChange={(next) => !next && requestClose()}>
      <AnimatePresence>
        {open && account ? (
          <EntranceGate key="sheet">
            <Dialog.Portal forceMount>
              <Dialog.Overlay asChild forceMount>
                <GatedMotion.div
                  // One layer below the shared Dialog, so its confirmations dim the sheet too.
                  className="fixed inset-0 z-30 bg-obsidian-950/55"
                  {...scrimMotion(!animate)}
                />
              </Dialog.Overlay>
              <Dialog.Content
                asChild
                forceMount
                aria-describedby={undefined}
                // The panel takes the focus, not the close button (its tooltip would pop open).
                onOpenAutoFocus={(event) => {
                  event.preventDefault();
                  (event.currentTarget as HTMLElement).focus({ preventScroll: true });
                }}
                // Toasts live outside the sheet: "Geri al" or "Ayrıntıları kopyala" there must not
                // close it.
                onPointerDownOutside={(event) => {
                  if ((event.target as Element | null)?.closest?.("[data-sonner-toaster]")) {
                    event.preventDefault();
                  }
                }}
                onFocusOutside={(event) => {
                  if ((event.target as Element | null)?.closest?.("[data-sonner-toaster]")) {
                    event.preventDefault();
                  }
                }}
              >
                <GatedMotion.aside
                  tabIndex={-1}
                  className="glass-strong fixed top-0 right-0 bottom-0 z-40 flex w-[min(880px,94vw)] flex-col shadow-[-30px_0_80px_rgb(34_6_6/0.5)] outline-none"
                  {...sheetMotion(!animate)}
                >
                  <header className="flex items-center gap-3.5 border-b border-white/8 px-5 py-4">
                    <PlayerHead name={account.username} src={account.headUrl} size={36} />
                    <div className="min-w-0 flex-1">
                      <Dialog.Title className="truncate font-display text-[18px] font-semibold text-bone-50">
                        {t("skins.title")}
                      </Dialog.Title>
                      <p className="truncate text-[12.5px] text-ash-400" data-selectable>
                        {view?.username ?? account.username}
                      </p>
                    </div>
                    <IconButton
                      label={t("common.close")}
                      icon={<X size={18} />}
                      size="sm"
                      onClick={requestClose}
                    />
                  </header>

                  {view && (history.data || history.isError) ? (
                    <Editor
                      key={account.id}
                      account={account}
                      view={view}
                      history={history.data ?? null}
                      onUpdate={(next) =>
                        queryClient.setQueryData(["skin-appearance", account.id], next)
                      }
                      onDirtyChange={(dirty) => {
                        dirtyRef.current = { id: account.id, dirty };
                      }}
                      onRetry={retry}
                      onSignIn={onSignIn}
                      refreshing={isFetching}
                    />
                  ) : isError ? (
                    <LoadError
                      error={error}
                      retrying={isFetching}
                      onRetry={() => void refetch()}
                      onSignIn={onSignIn}
                    />
                  ) : (
                    <SheetSkeleton />
                  )}

                  <ConfirmDialog
                    open={confirmOpen}
                    onOpenChange={setConfirmOpen}
                    title={t("skins.discard.title")}
                    description={t("skins.discard.body")}
                    closeLabel={t("common.close")}
                    width={440}
                    footer={
                      <>
                        <Button variant="ghost" onClick={() => setConfirmOpen(false)}>
                          {t("skins.discard.keep")}
                        </Button>
                        <Button
                          variant="danger"
                          onClick={() => {
                            setConfirmOpen(false);
                            onClose();
                          }}
                        >
                          {t("skins.discard.confirm")}
                        </Button>
                      </>
                    }
                  />
                </GatedMotion.aside>
              </Dialog.Content>
            </Dialog.Portal>
          </EntranceGate>
        ) : null}
      </AnimatePresence>
    </Dialog.Root>
  );
}

/**
 * The "Görünüm" panel of a Microsoft account: a side sheet with a 3D preview. Only Microsoft
 * accounts open it (offline accounts pick a bundled picture in the account center instead);
 * `account` null closes it.
 */
export function AppearanceSheet({
  account,
  onClose,
  onSignIn,
}: {
  account: AccountSummary | null;
  onClose: () => void;
  /** Opens the Microsoft sign-in again (an expired session). */
  onSignIn: () => void;
}) {
  return (
    <AppearanceSheetPanel
      account={account?.kind === "microsoft" ? account : null}
      onClose={onClose}
      onSignIn={onSignIn}
    />
  );
}
