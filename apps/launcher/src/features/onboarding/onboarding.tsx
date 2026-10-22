import {
  ArrowLeft,
  Check,
  GlobeHemisphereWest,
  Image as ImageIcon,
  Robot,
  User,
  UserPlus,
} from "@phosphor-icons/react";
import {
  Button,
  Chip,
  cn,
  EmberField,
  Field,
  Input,
  Skeleton,
  SwapLabel,
  Switch,
  toast,
  useMotionPrefs,
} from "@pl/ui";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { AnimatePresence, motion } from "motion/react";
import {
  type ReactNode,
  type RefObject,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import heroArt from "@/assets/art/hero-dragon-1.webp";
import logo from "@/assets/brand/logo-96.webp";
import { AccountFace } from "@/features/accounts/account-face";
import { sortAccounts } from "@/features/accounts/account-order";
import { checkOfflineName } from "@/features/accounts/name-rules";
import { useImportJob } from "@/features/import/import-job";
import { ImportStatusCard } from "@/features/import/import-status";
import { ImportStep } from "@/features/import/import-step";
import { suggestedName } from "@/features/import/model";
import { importKeys } from "@/features/import/use-import-picker";
import { MigrationDialog } from "@/features/migrate/migration-dialog";
import { MicrosoftLoginDialog } from "@/features/settings/accounts-section";
import { MicrosoftMark } from "@/features/settings/microsoft-mark";
import { api, type InstanceSummary, type LegacyScan } from "@/lib/api";
import { applyStartOnBoot } from "@/lib/autostart";
import { listFormat, uiLocale } from "@/lib/format";
import { isTauri } from "@/lib/platform";
import { InstanceIcon } from "@/shell/instance-icon";
import { TitleBar } from "@/shell/title-bar";
import { usePrefs } from "@/stores/prefs";

type Step = "welcome" | "legacy" | "import" | "account" | "projects";

const enter = { duration: 0.32, ease: [0.16, 1, 0.3, 1] as const };

/**
 * The block is centred, so a step of another height would move the logo and the progress bar
 * in one jump. When its height changes, it glides from where it was to its new place instead
 * (transform only, started before the frame paints; an interrupted glide continues from where
 * it is). Off in Performance mode and under reduced motion: the block simply moves.
 */
function useGlideOnResize(ref: RefObject<HTMLElement | null>, enabled: boolean) {
  useLayoutEffect(() => {
    const element = ref.current;
    if (!element || !enabled || typeof ResizeObserver === "undefined") return;
    let top = element.offsetTop;
    let glide: Animation | null = null;
    const observer = new ResizeObserver(() => {
      const next = element.offsetTop;
      const moved = top - next;
      top = next;
      if (moved === 0) return;
      const now = glide ? new DOMMatrixReadOnly(getComputedStyle(element).transform).m42 : 0;
      glide?.cancel();
      glide = element.animate(
        [{ transform: `translateY(${moved + now}px)` }, { transform: "translateY(0px)" }],
        { duration: 340, easing: "cubic-bezier(0.16, 1, 0.3, 1)" },
      );
    });
    observer.observe(element);
    return () => {
      observer.disconnect();
      glide?.cancel();
    };
  }, [ref, enabled]);
}

/** Shown until the first run is finished: in the app always, in the browser preview on request. */
export function shouldShowOnboarding(onboarded: boolean): boolean {
  if (onboarded) return false;
  return isTauri() || new URLSearchParams(window.location.search).has("onboarding");
}

/**
 * A running or finished import from another launcher, in a glass card at the bottom right, over
 * the art and clear of the steps (where the launcher's toasts appear): the flow goes on while it
 * copies. It rises in when the player leaves the import step and stays until the flow ends.
 */
function ImportCardSlot({ hidden }: { hidden: boolean }) {
  const status = useImportJob((s) => s.status);
  const motionPrefs = useMotionPrefs();
  const shown = status !== "idle" && !hidden;
  return (
    <div className="pointer-events-none absolute right-6 bottom-6 w-[320px]">
      <AnimatePresence>
        {shown ? (
          <motion.div
            key="import"
            className="pointer-events-auto"
            initial={{ opacity: 0, transform: motionPrefs.reduce ? "none" : "translateY(10px)" }}
            animate={{ opacity: 1, transform: "translateY(0px)", transition: enter }}
            exit={{ opacity: 0, transition: { duration: 0.18 } }}
          >
            <ImportStatusCard />
          </motion.div>
        ) : null}
      </AnimatePresence>
    </div>
  );
}

/**
 * First run: welcome, optional v1 import, an optional import from another launcher (IMPORT1), an
 * account, and the Projects instance. Every step can be left for later; nothing here is required
 * to reach the launcher.
 */
export function Onboarding() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const set = usePrefs((s) => s.set);
  const selectInstance = usePrefs((s) => s.selectInstance);
  const particlesOn = usePrefs((s) => s.particles);
  const { data: scan, isPending: scanning } = useQuery({
    queryKey: ["legacy-scan"],
    queryFn: () => api.legacyScan(),
  });
  // Other launchers (small files and folder listings): their step is known before "Başlayalım".
  const { data: imports, isPending: importScanning } = useQuery({
    queryKey: importKeys.scan,
    queryFn: () => api.importer.scan(),
  });
  // Read early for the last step (its switch's description), so nothing moves there.
  useQuery({ queryKey: ["afk"], queryFn: () => api.afkAccounts() });
  const steps: Step[] = [
    "welcome",
    ...(scan?.found ? (["legacy"] as const) : []),
    ...(imports?.sources.length ? (["import"] as const) : []),
    "account",
    "projects",
  ];
  const [step, setStep] = useState<Step>("welcome");
  const index = steps.indexOf(step);
  const headingRef = useRef<HTMLHeadingElement>(null);
  const sectionRef = useRef<HTMLElement>(null);
  const motionPrefs = useMotionPrefs();
  useGlideOnResize(sectionRef, motionPrefs.layout);

  // Move focus to the new step's title so keyboard and screen reader users follow along.
  useEffect(() => {
    if (step !== "welcome") headingRef.current?.focus();
  }, [step]);

  const go = (offset: number) => {
    const next = steps[index + offset];
    if (next) setStep(next);
  };

  const finish = (instance?: InstanceSummary) => {
    if (instance) selectInstance(instance.id);
    set("legacyOffered", true);
    set("onboarded", true);
    void navigate({ to: "/" });
  };

  const skip = () => {
    // The Projects instance is only metadata until the first launch, so create it anyway (not
    // when this data folder's player deleted it: the library offers it, LIB1).
    void api
      .ensureOfficialInstance()
      .then((instance) => {
        if (instance && !usePrefs.getState().selectedInstanceId) selectInstance(instance.id);
      })
      .catch(() => {});
    finish();
  };

  return (
    <div className="fixed inset-0 z-40 flex flex-col overflow-hidden bg-obsidian-950 text-bone-100">
      <div aria-hidden className="pointer-events-none absolute inset-0">
        <img
          src={heroArt}
          alt=""
          draggable={false}
          className="size-full object-cover object-[80%_center] opacity-90"
        />
        <div className="absolute inset-0 bg-[linear-gradient(90deg,rgb(10_9_12/0.97)_0%,rgb(10_9_12/0.9)_38%,rgb(34_6_6/0.45)_64%,transparent_100%)]" />
        <div className="absolute inset-x-0 bottom-0 h-48 bg-[linear-gradient(to_top,rgb(10_9_12/0.85),transparent)]" />
        <EmberField kind={particlesOn ? "embers" : "none"} />
      </div>

      <div className="relative">
        <TitleBar search={false} />
      </div>

      <div className="relative flex min-h-0 flex-1 items-center px-[clamp(32px,7vw,96px)] pb-10">
        <section
          ref={sectionRef}
          aria-labelledby="onboarding-title"
          className="w-full max-w-[500px]"
        >
          <div className="mb-9 flex items-center gap-4">
            <img src={logo} alt="" className="size-11" draggable={false} />
            {/* The step count waits for the old launcher scan (it may add a step), so it never
                changes from "1 / 3" to "1 / 4" under the player's eyes; it fades in instead. */}
            <div
              className={cn(
                "flex-1 transition-opacity duration-300",
                scanning || importScanning ? "opacity-0" : "opacity-100",
              )}
            >
              <p className="text-[12.5px] font-medium text-ash-400">
                {t("onboarding.progress", { current: index + 1, total: steps.length })}
              </p>
              <div className="mt-2 flex gap-1.5" aria-hidden>
                {steps.map((s, i) => (
                  <span
                    key={s}
                    className={cn(
                      "h-1 flex-1 rounded-full transition-colors duration-300",
                      i <= index ? "bg-gold-500" : "bg-white/12",
                    )}
                  />
                ))}
              </div>
            </div>
          </div>

          <AnimatePresence mode="wait" initial={false}>
            <motion.div
              key={step}
              initial={{ opacity: 0, y: 14 }}
              animate={{ opacity: 1, y: 0, transition: enter }}
              exit={{ opacity: 0, y: -8, transition: { duration: 0.18 } }}
            >
              {step === "welcome" ? (
                <StepFrame
                  headingRef={headingRef}
                  title={t("onboarding.welcome.title")}
                  body={t("onboarding.welcome.body")}
                  hero
                >
                  <div className="mt-9 flex items-center gap-3">
                    <Button variant="primary" size="lg" onClick={() => go(1)}>
                      {t("onboarding.welcome.start")}
                    </Button>
                    <Button variant="ghost" onClick={skip}>
                      {t("onboarding.skip")}
                    </Button>
                  </div>
                </StepFrame>
              ) : null}
              {step === "legacy" && scan ? (
                <LegacyStep
                  headingRef={headingRef}
                  scan={scan}
                  onBack={() => go(-1)}
                  onNext={() => go(1)}
                />
              ) : null}
              {step === "import" && imports ? (
                <StepFrame
                  headingRef={headingRef}
                  title={t("import.step.title")}
                  body={t("import.step.body")}
                >
                  <ImportStep
                    scan={imports}
                    back={<BackButton onClick={() => go(-1)} />}
                    onNext={() => go(1)}
                  />
                </StepFrame>
              ) : null}
              {step === "account" ? (
                <AccountStep
                  headingRef={headingRef}
                  suggestedName={suggestedName(imports)}
                  onBack={() => go(-1)}
                  onNext={() => go(1)}
                />
              ) : null}
              {step === "projects" ? (
                <ProjectsStep headingRef={headingRef} onBack={() => go(-1)} onFinish={finish} />
              ) : null}
            </motion.div>
          </AnimatePresence>
        </section>
      </div>
      <ImportCardSlot hidden={step === "import"} />
    </div>
  );
}

function StepFrame({
  headingRef,
  title,
  body,
  hero,
  children,
}: {
  headingRef: RefObject<HTMLHeadingElement | null>;
  title: string;
  body: string;
  hero?: boolean;
  children: ReactNode;
}) {
  return (
    <div>
      <h1
        id="onboarding-title"
        ref={headingRef}
        tabIndex={-1}
        className={cn(
          "font-display font-semibold text-bone-50 outline-none",
          hero ? "text-hero tracking-[-0.01em]" : "text-[28px] leading-[1.08]",
        )}
      >
        {title}
      </h1>
      <p className="mt-3 max-w-[46ch] text-[15px] leading-[1.55] text-ash-300">{body}</p>
      {children}
    </div>
  );
}

function BackButton({ onClick }: { onClick: () => void }) {
  const { t } = useTranslation();
  return (
    <Button variant="ghost" icon={<ArrowLeft size={16} />} onClick={onClick}>
      {t("onboarding.back")}
    </Button>
  );
}

function LegacyStep({
  headingRef,
  scan,
  onBack,
  onNext,
}: {
  headingRef: RefObject<HTMLHeadingElement | null>;
  scan: LegacyScan;
  onBack: () => void;
  onNext: () => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [open, setOpen] = useState(false);
  // Each line found, and its words in the sentence above the list ("oyuncu adını").
  const items: Array<{ icon: ReactNode; text: string; thing: string }> = [];
  if (scan.nickname)
    items.push({
      icon: <User size={18} />,
      text: t("onboarding.legacy.nickname", { name: scan.nickname }),
      thing: t("onboarding.legacy.things.nickname"),
    });
  if (scan.afkAccounts > 0)
    items.push({
      icon: <Robot size={18} />,
      text: t("onboarding.legacy.afk", { count: scan.afkAccounts }),
      thing: t("onboarding.legacy.things.afk"),
    });
  if (scan.screenshots > 0)
    items.push({
      icon: <ImageIcon size={18} />,
      text: t("onboarding.legacy.screenshots", { count: scan.screenshots }),
      thing: t("onboarding.legacy.things.screenshots"),
    });
  if (scan.worlds > 0)
    items.push({
      icon: <GlobeHemisphereWest size={18} />,
      text: t("onboarding.legacy.worlds", { count: scan.worlds }),
      thing: t("onboarding.legacy.things.worlds"),
    });
  // The sentence names only what was found (it promised worlds to players who had none).
  const things = items.length
    ? items.map((item) => item.thing)
    : [t("onboarding.legacy.things.data")];
  const body = t("onboarding.legacy.body", { things: listFormat().format(things) });

  return (
    <StepFrame
      headingRef={headingRef}
      title={t("onboarding.legacy.title")}
      body={body.charAt(0).toLocaleUpperCase(uiLocale()) + body.slice(1)}
    >
      {items.length ? (
        <ul className="mt-6 space-y-2.5">
          {items.map((item) => (
            <li key={item.text} className="flex items-center gap-3 text-[14.5px] text-bone-100">
              <span className="text-gold-300">{item.icon}</span>
              {item.text}
            </li>
          ))}
        </ul>
      ) : null}
      <div className="mt-9 flex flex-wrap items-center gap-3">
        <Button variant="primary" size="lg" onClick={() => setOpen(true)}>
          {t("onboarding.legacy.import")}
        </Button>
        <Button variant="secondary" onClick={onNext}>
          {t("onboarding.legacy.skip")}
        </Button>
        <BackButton onClick={onBack} />
      </div>
      <MigrationDialog
        open={open}
        onOpenChange={setOpen}
        // Only a finished import moves on; closing the dialog without one keeps this step, where
        // "Aktarmadan devam et" is.
        onFinished={(imported) => {
          if (!imported) return;
          void queryClient.invalidateQueries({ queryKey: ["accounts"] });
          void queryClient.invalidateQueries({ queryKey: ["instances"] });
          void queryClient.invalidateQueries({ queryKey: ["legacy-scan"] });
          onNext();
        }}
      />
    </StepFrame>
  );
}

function AccountStep({
  headingRef,
  suggestedName,
  onBack,
  onNext,
}: {
  headingRef: RefObject<HTMLHeadingElement | null>;
  /** The offline name another launcher used (TLauncher), to start the name field with. */
  suggestedName?: string;
  onBack: () => void;
  onNext: () => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { data: accounts, isPending } = useQuery({
    queryKey: ["accounts"],
    queryFn: () => api.accounts(),
  });
  const [adding, setAdding] = useState(false);
  const [loginOpen, setLoginOpen] = useState(false);
  const [name, setName] = useState(suggestedName ?? "");
  const [final, setFinal] = useState(false);
  const nameInput = useRef<HTMLInputElement>(null);

  const hasAccount = Boolean(accounts?.length);
  // The name rules show as the player types (Turkish letters, spaces, a name already used);
  // "too short" waits for "Bu adla devam et", as in the account center.
  const check = checkOfflineName(name, accounts ?? [], { final });
  const nameError = check.problem ? t(`accounts.name.${check.problem}`) : null;
  const addOffline = useMutation({
    mutationFn: ({ name: n }: { name: string; first: boolean }) => api.addOfflineAccount(n),
    onSuccess: (_, { first }) => {
      if (!first) {
        setName("");
        setFinal(false);
        setAdding(false);
        void queryClient.invalidateQueries({ queryKey: ["accounts"] });
        return;
      }
      // "Bu adla devam et" keeps its word: the first account moves straight on. The list is read
      // again once this step has left, so it does not change while it fades out.
      onNext();
      window.setTimeout(() => void queryClient.invalidateQueries({ queryKey: ["accounts"] }), 300);
    },
    onError: (error) => toast.error(error instanceof Error ? error.message : String(error)),
  });

  const submitOffline = () => {
    const strict = checkOfflineName(name, accounts ?? [], { final: true });
    setFinal(true);
    if (strict.problem) return;
    addOffline.mutate({ name: strict.name, first: !hasAccount });
  };

  const showForms = !hasAccount || adding;

  return (
    <StepFrame
      headingRef={headingRef}
      title={hasAccount && !adding ? t("onboarding.account.ready") : t("onboarding.account.title")}
      body={
        hasAccount && !adding ? t("onboarding.account.readyBody") : t("onboarding.account.body")
      }
    >
      {isPending ? (
        <Skeleton className="mt-7 h-16 w-full" />
      ) : hasAccount && !adding ? (
        <ul className="mt-7 space-y-2">
          {sortAccounts(accounts ?? []).map((a) => (
            <li
              key={a.id}
              className={cn(
                "flex items-center gap-3.5 rounded-[var(--radius-panel)] bg-obsidian-900/70 px-3.5 py-3 ring-1",
                a.active ? "ring-gold-400/40" : "ring-white/8",
              )}
            >
              <AccountFace account={a} size={40} />
              <div className="min-w-0 flex-1">
                <p className="truncate text-[15px] font-semibold text-bone-50">{a.username}</p>
                {a.kind === "microsoft" ? (
                  <p className="text-[12.5px] text-ash-400">{t("accounts.microsoft")}</p>
                ) : null}
              </div>
              {a.active ? <Check size={18} className="text-gold-300" aria-hidden /> : null}
            </li>
          ))}
        </ul>
      ) : null}

      {showForms ? (
        <div className="mt-7">
          <Button
            variant={hasAccount ? "secondary" : "primary"}
            size="lg"
            icon={<MicrosoftMark size={16} />}
            onClick={() => setLoginOpen(true)}
          >
            {t("onboarding.account.microsoft")}
          </Button>
          <div className="my-6 flex items-center gap-3 text-[12.5px] text-ash-400">
            <span className="h-px flex-1 bg-white/10" />
            {t("onboarding.account.or")}
            <span className="h-px flex-1 bg-white/10" />
          </div>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              submitOffline();
            }}
          >
            <Field label={t("onboarding.account.nameLabel")} htmlFor="onboarding-name">
              <div className="flex gap-2">
                <Input
                  ref={nameInput}
                  id="onboarding-name"
                  value={name}
                  invalid={Boolean(nameError)}
                  aria-describedby="onboarding-name-problem"
                  maxLength={16}
                  autoComplete="off"
                  spellCheck={false}
                  onChange={(e) => {
                    setName(e.target.value);
                    setFinal(false);
                  }}
                />
                <Button
                  type="submit"
                  variant="secondary"
                  icon={<UserPlus size={16} />}
                  disabled={addOffline.isPending}
                >
                  {t("onboarding.account.useName")}
                </Button>
              </div>
              {/* Only problems are spelled out, with a fixed name to take when there is one ("Çağrı"
                  → "Cagri kullan"); the line keeps its height, so the step never moves. */}
              <p
                id="onboarding-name-problem"
                aria-live="polite"
                className="min-h-[18px] text-[12.5px] leading-snug text-flare-400"
              >
                {nameError}
                {nameError && check.suggestion ? (
                  <>
                    {" "}
                    <button
                      type="button"
                      className="cursor-pointer font-semibold text-gold-300 underline-offset-2 hover:underline focus-visible:underline"
                      onClick={() => {
                        setName(check.suggestion ?? name);
                        nameInput.current?.focus();
                      }}
                    >
                      {t("accounts.center.useSuggestion", { name: check.suggestion })}
                    </button>
                  </>
                ) : null}
              </p>
            </Field>
          </form>
        </div>
      ) : null}

      <div className="mt-9 flex flex-wrap items-center gap-3">
        {hasAccount ? (
          <Button variant="primary" size="lg" onClick={onNext}>
            {t("onboarding.account.continue")}
          </Button>
        ) : (
          <Button variant="secondary" onClick={onNext}>
            {t("onboarding.skip")}
          </Button>
        )}
        {hasAccount && !adding ? (
          <Button variant="ghost" icon={<UserPlus size={16} />} onClick={() => setAdding(true)}>
            {t("onboarding.account.addAnother")}
          </Button>
        ) : null}
        <BackButton onClick={onBack} />
      </div>
      <MicrosoftLoginDialog
        open={loginOpen}
        onOpenChange={(open) => {
          setLoginOpen(open);
          if (!open) setAdding(false);
        }}
      />
    </StepFrame>
  );
}

function ProjectsStep({
  headingRef,
  onBack,
  onFinish,
}: {
  headingRef: RefObject<HTMLHeadingElement | null>;
  onBack: () => void;
  onFinish: (instance?: InstanceSummary) => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const prefs = usePrefs();
  const ensure = useMutation({
    mutationFn: () => api.ensureOfficialInstance(),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["instances"] });
      void queryClient.invalidateQueries({ queryKey: ["official-status"] });
    },
  });
  // This data folder's player had deleted the Projects instance (a reinstall that cleared the
  // launcher's own settings shows this flow again): it is offered, not made (LIB1).
  const install = useMutation({
    mutationFn: () => api.installOfficialInstance(),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["instances"] });
      void queryClient.invalidateQueries({ queryKey: ["official-status"] });
    },
  });
  const { data: official } = useQuery({
    queryKey: ["official-status"],
    queryFn: () => api.officialStatus(),
  });
  // "Bilgisayar açılınca başlat" says what it is for only to a player with AFK accounts (an
  // import may have brought them); for the others the label is enough.
  const { data: afk } = useQuery({ queryKey: ["afk"], queryFn: () => api.afkAccounts() });
  const { mutate } = ensure;
  const started = useRef(false);
  useEffect(() => {
    // Once per visit (effects run twice in development).
    if (started.current) return;
    started.current = true;
    mutate();
  }, [mutate]);

  const instance = install.data ?? ensure.data ?? undefined;
  // `null` from the backend: deleted here before, so "Kur" waits for the player.
  const offered = ensure.data === null && !install.data;
  const failed = ensure.isError || install.isError;
  const installing = install.isPending;
  const pack = official?.pack;
  return (
    <StepFrame
      headingRef={headingRef}
      title={
        instance
          ? t("onboarding.projects.title")
          : offered
            ? t("lib1.empty.question")
            : t("onboarding.projects.preparing")
      }
      body={t("onboarding.projects.body")}
    >
      <div className="mt-7">
        {failed ? (
          <div className="flex items-center gap-3">
            <p className="text-[14px] text-flare-400" role="alert">
              {t("onboarding.projects.failed")}
            </p>
            <Button
              size="sm"
              variant="secondary"
              onClick={() => (install.isError ? install.mutate() : mutate())}
            >
              {t("common.retry")}
            </Button>
          </div>
        ) : offered ? (
          // The pack as the installed card would show it, dimmed, with its one action.
          <div className="flex items-center gap-4 rounded-[var(--radius-panel)] bg-obsidian-900/50 px-4 py-3.5 ring-1 ring-white/10">
            <InstanceIcon icon="dragon-egg" size={56} className="opacity-60 saturate-[0.6]" />
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <p className="truncate font-display text-[19px] font-semibold text-bone-50/85">
                  {pack?.name ?? "Projects"}
                </p>
                <Chip tone="gold">{t("instances.managed")}</Chip>
              </div>
              <p className="mt-0.5 text-[13px] text-ash-400">
                {pack
                  ? `Minecraft ${pack.mcVersion}, ${t(`loaders.${pack.loader}`)}`
                  : t("lib1.notInstalled")}
              </p>
            </div>
            <Button
              variant="secondary"
              aria-label={t("lib1.install")}
              aria-busy={installing || undefined}
              onClick={() => {
                if (!installing) install.mutate();
              }}
            >
              <SwapLabel
                labels={[t("lib1.installShort"), t("lib1.installing")]}
                active={installing ? 1 : 0}
              />
            </Button>
          </div>
        ) : instance ? (
          <div className="flex items-center gap-4 rounded-[var(--radius-panel)] bg-obsidian-900/70 px-4 py-3.5 ring-1 ring-gold-400/30">
            <InstanceIcon icon={instance.icon} size={56} />
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <p className="truncate font-display text-[19px] font-semibold text-bone-50">
                  {instance.name}
                </p>
                {instance.managed ? <Chip tone="gold">{t("instances.managed")}</Chip> : null}
              </div>
              <p className="mt-0.5 text-[13px] text-ash-400">
                Minecraft {instance.mcVersion}, {t(`loaders.${instance.loader}`)}
              </p>
            </div>
          </div>
        ) : (
          <Skeleton className="h-[84px] w-full" />
        )}
        {instance || offered ? (
          <p className="mt-3 text-[13px] text-ash-400">
            {official?.pack
              ? t("onboarding.projects.pack", { name: official.pack.name })
              : t("onboarding.projects.noPack")}
          </p>
        ) : failed ? null : (
          // Holds the line's place while the instance is prepared, so the switches below stay.
          <Skeleton className="mt-3 h-[19px] w-3/4" />
        )}
      </div>

      <div className="mt-7 space-y-4">
        <Switch
          checked={prefs.startOnBoot}
          onCheckedChange={(v) => {
            prefs.set("startOnBoot", v);
            applyStartOnBoot(v).catch(() => {
              prefs.set("startOnBoot", !v);
              toast.error(t("settings.general.startOnBootFailed"));
            });
          }}
          label={t("onboarding.projects.startOnBoot")}
          description={afk?.length ? t("onboarding.projects.startOnBootHelp") : undefined}
        />
        <Switch
          checked={prefs.discordEnabled}
          onCheckedChange={(v) => {
            prefs.set("discordEnabled", v);
            void api.setPresenceEnabled(v);
          }}
          label={t("onboarding.projects.discord")}
        />
      </div>

      <div className="mt-9 flex flex-wrap items-center gap-3">
        <Button
          variant="primary"
          size="lg"
          disabled={!instance && !offered}
          onClick={() => onFinish(instance)}
        >
          {t("onboarding.projects.finish")}
        </Button>
        <BackButton onClick={onBack} />
      </div>
    </StepFrame>
  );
}
