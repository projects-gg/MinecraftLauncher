import type { Plugin } from "vite";

/**
 * The Phosphor icon weights the launcher draws. Every icon module of `@phosphor-icons/react`
 * builds all six weights (thin, light, regular, bold, fill, duotone) as React elements the moment
 * it loads. The launcher never asks for thin or light, so they are dropped at build time: about
 * 120 KB less script to parse at every start and a third fewer elements built. A weight missing
 * here draws nothing, so `phosphor-weights.test.ts` fails when the code asks for one.
 */
export const USED_WEIGHTS = ["regular", "bold", "fill", "duotone"] as const;

const ALL_WEIGHTS = ["thin", "light", "regular", "bold", "fill", "duotone"];
const DROPPED = ALL_WEIGHTS.filter((w) => !(USED_WEIGHTS as readonly string[]).includes(w));

/** An icon definition module (`dist/defs/<Icon>.es.js`, a Map from weight to element). */
const DEFINITION = /@phosphor-icons[\\/]react[\\/]dist[\\/]defs[\\/][^\\/]+\.es\.js$/;
/** One `[ "<weight>", <element> ]` entry of that Map, as the package prints it. */
const DROPPED_ENTRY = new RegExp(
  `\\n {2}\\[\\n {4}"(?:${DROPPED.join("|")})",\\n[\\s\\S]*?\\n {2}\\],?(?=\\n)`,
  "g",
);

/** The definition module without the weights the launcher never draws. */
export function dropUnusedWeights(code: string): string {
  return code.replace(DROPPED_ENTRY, "");
}

export function phosphorWeights(): Plugin {
  return {
    name: "pl-phosphor-weights",
    enforce: "pre",
    transform(code, id) {
      if (!DEFINITION.test(id)) return null;
      const out = dropUnusedWeights(code);
      return out === code ? null : { code: out, map: null };
    },
  };
}
