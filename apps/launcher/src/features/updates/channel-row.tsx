// Settings → Güncellemeler "Güncelleme kanalı" (BETA-GATE): Kararlı and Beta, both always there.
// Beta asks first (beta-gate.ts): the plate goes to Beta while the answer is out and stays when it
// lets the launcher in; otherwise it glides back to Kararlı and one line under the title says why,
// with what to do (apply to the team, sign in, try again). The lines take turns in one grid cell,
// as the overlay's do, so changing between them never moves the rows below.
import { ArrowSquareOut, Warning } from "@phosphor-icons/react";
import { cn, Segmented } from "@pl/ui";
import { type ReactNode, useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { Row } from "@/features/settings/row";
import { useSocialUi } from "@/features/social/store";
import { openWebLink } from "@/lib/links";
import { type UpdateChannel, usePrefs } from "@/stores/prefs";
import { useUpdate } from "@/stores/update";
import { useBetaGate } from "./beta-gate";

/** The answer is out this long before "Ar-Ge rolüne bakılıyor" shows (a quick one never flashes). */
const SLOW_MS = 250;

type Line = "beta" | "checking" | "notMember" | "signIn" | "unknown";
const LINES: Line[] = ["beta", "checking", "notMember", "signIn", "unknown"];

/** True once `on` has lasted `ms`; false again as soon as it ends. */
function useLasted(on: boolean, ms: number): boolean {
  const [lasted, setLasted] = useState(false);
  useEffect(() => {
    if (!on) {
      setLasted(false);
      return;
    }
    const timer = setTimeout(() => setLasted(true), ms);
    return () => clearTimeout(timer);
  }, [on, ms]);
  return on && lasted;
}

/**
 * A line's action: gold words right after its sentence, as the overlay's lines carry theirs.
 * `joined`: the words finish the sentence ("… Ekipteysen Discord ile giriş yap"), so a word space
 * leads them; `external`: they open the browser, so they carry the launcher's external-link mark.
 */
function LineAction({
  shown,
  onPress,
  joined,
  external,
  children,
}: {
  shown: boolean;
  onPress: () => void;
  joined?: boolean;
  external?: boolean;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      tabIndex={shown ? undefined : -1}
      onClick={onPress}
      className={cn(
        "relative cursor-pointer rounded-sm font-medium text-gold-300 underline decoration-transparent underline-offset-2 outline-none transition-[text-decoration-color,scale] duration-150 hover:decoration-current focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.97]",
        joined ? "ml-1" : "ml-1.5",
        shown && "hit-area",
      )}
    >
      {children}
      {external ? (
        <ArrowSquareOut
          size={12}
          weight="bold"
          aria-hidden
          className="ml-1 inline-block align-[-1px]"
        />
      ) : null}
    </button>
  );
}

export function UpdateChannelRow() {
  const { t } = useTranslation();
  const channel = usePrefs((s) => s.updateChannel);
  const setPref = usePrefs((s) => s.set);
  const asking = useBetaGate((s) => s.asking);
  const refusal = useBetaGate((s) => s.refusal);
  const slow = useLasted(asking, SLOW_MS);
  const lineId = useId();

  // While this row is on screen its line says why a look left Beta: no toast says it again.
  useEffect(() => {
    useBetaGate.setState((s) => ({ rowsShown: s.rowsShown + 1 }));
    return () => useBetaGate.setState((s) => ({ rowsShown: s.rowsShown - 1 }));
  }, []);

  const join = async () => {
    // The answer lets this launcher in: a look at the beta channel at once, in the background.
    if (await useBetaGate.getState().join()) void useUpdate.getState().check();
  };
  const pick = (value: UpdateChannel) => {
    if (value === "beta") {
      void join();
      return;
    }
    useBetaGate.getState().clear();
    setPref("updateChannel", "stable");
  };

  // While the answer is out the plate is on Beta (then "bakılıyor" if slow). One line in every
  // state, so the row keeps its height: what Beta is, unless an answer said why not.
  const shownChannel: UpdateChannel = asking ? "beta" : channel;
  const line: Line = asking
    ? slow
      ? "checking"
      : "beta"
    : channel !== "beta" && refusal && refusal.verdict !== "allowed"
      ? refusal.verdict
      : "beta";

  const sentence: Record<Line, string> = {
    beta: t("settings.updates.betaHelp"),
    checking: t("betaGate.checking"),
    notMember: t("betaGate.notMember"),
    signIn: t("betaGate.signIn"),
    unknown: t("betaGate.unknown"),
  };
  const apply = refusal?.apply;
  const action = (kind: Line, shown: boolean): ReactNode => {
    if (kind === "notMember" && apply) {
      return (
        <LineAction shown={shown} external onPress={() => openWebLink(apply)}>
          {t("betaGate.apply")}
        </LineAction>
      );
    }
    if (kind === "signIn") {
      return (
        <LineAction shown={shown} joined onPress={() => useSocialUi.getState().openPanel()}>
          {t("betaGate.signInAction")}
        </LineAction>
      );
    }
    if (kind === "unknown") {
      return (
        <LineAction shown={shown} onPress={() => void join()}>
          {t("betaGate.retry")}
        </LineAction>
      );
    }
    return null;
  };

  return (
    <Row
      title={t("settings.updates.channel")}
      helpId={lineId}
      help={
        <>
          {/* Heard when an answer changes the line: one whole sentence (the sign-in line's
                ends in its action's words). */}
          <span className="sr-only" aria-live="polite" aria-atomic="true">
            {line === "signIn"
              ? `${sentence.signIn} ${t("betaGate.signInAction")}.`
              : sentence[line]}
          </span>
          <span className="grid">
            {LINES.map((kind) => {
              const shown = kind === line;
              return (
                <span
                  key={kind}
                  inert={!shown}
                  className={cn(
                    "[grid-area:1/1] whitespace-nowrap transition-opacity",
                    shown ? "opacity-100 delay-[60ms] duration-150" : "opacity-0 duration-100",
                  )}
                >
                  {/* Only a check that failed takes the launcher's warning mark: "Beta is for
                        the team" is an invitation, not an error. */}
                  {kind === "unknown" ? (
                    <Warning
                      size={13}
                      weight="fill"
                      aria-hidden
                      className="mr-1.5 inline-block align-[-2px] text-gold-300"
                    />
                  ) : null}
                  <span aria-hidden>{sentence[kind]}</span>
                  {action(kind, shown)}
                  {/* The sign-in words finish their sentence. */}
                  {kind === "signIn" ? <span aria-hidden>.</span> : null}
                </span>
              );
            })}
          </span>
        </>
      }
    >
      <Segmented
        value={shownChannel}
        onValueChange={pick}
        label={t("settings.updates.channel")}
        describedBy={lineId}
        options={[
          { value: "stable", label: t("settings.updates.stable") },
          { value: "beta", label: t("settings.updates.beta") },
        ]}
      />
    </Row>
  );
}
