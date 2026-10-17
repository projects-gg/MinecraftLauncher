// Java (J1): Settings → Java and the question before a launch with a custom Java.

export type JavaRuntimeKind = "managed" | "system" | "custom";

/** An instance that uses a Java. */
export interface JavaUser {
  instanceId: string;
  name: string;
  /** Uses it through "Otomatik". */
  automatic: boolean;
  /** The Java does not suit the instance's game (custom paths only). */
  fit?: "tooOld" | "tooNew";
}

export interface JavaRuntime {
  /** `managed:<component>/<platform>` or `path:<executable>`. */
  id: string;
  kind: JavaRuntimeKind;
  major?: number;
  /** Full version (`21.0.7`, `1.8.0_51`). */
  version?: string;
  /** A familiar vendor name (`Eclipse Temurin`, `Microsoft`, `Mojang`). */
  vendor?: string;
  /** The Java folder. */
  location: string;
  executable: string;
  /** A 32-bit Java on a 64-bit computer. */
  bits32: boolean;
  users: JavaUser[];
  /** Launcher runtimes: Mojang's component and platform key. */
  component?: string;
  platform?: string;
  /** Launcher runtimes: the newer build Mojang offers. */
  update?: string;
  /** Launcher runtimes: the update is downloaded and waits for the game to close. */
  staged: boolean;
  /** Launcher runtimes: asked to update; starts once no game runs. */
  queued: boolean;
  /** Launcher runtimes: the update downloading now (percent). */
  updating?: number;
  /** Launcher runtimes: the last update failed. */
  updateFailed: boolean;
  /** Custom paths: gone or not working. */
  problem?: "missing" | "broken";
  /** Launcher runtimes no instance uses: "Kaldır" frees this many bytes. */
  removableBytes?: number;
}

export interface JavaOverview {
  runtimes: JavaRuntime[];
  /** Looking for Javas on the computer did not finish in time. */
  systemIncomplete: boolean;
  /** When Mojang was last asked for runtime updates (ms). */
  checkedAt?: number;
  /** A game runs or is being prepared: updates wait and nothing is removed. */
  gameActive: boolean;
}

/** The question before a launch whose custom Java does not suit the game. */
export interface JavaPrompt {
  instanceId: string;
  instanceName: string;
  problem: "tooOld" | "tooNew" | "missing" | "broken";
  /** The custom Java's version, when it runs. */
  have?: number;
  /** The Java the game needs (the maximum when the custom one is too new). */
  need: number;
  /** The mods raised the minimum above the game's own. */
  forMods: boolean;
  /** "Kur ve oyna" can fix it. */
  fixable: boolean;
  /** "Kur ve oyna" switches the instance to "Otomatik" (else to the launcher's Java `need`). */
  automatic: boolean;
}

/** The answer to the question, sent with the next launch. */
export type JavaChoice = "install" | "tryAnyway";

/** A Java picked for an instance ("Özel"): what it is and whether it suits the game. */
export interface JavaCheck {
  /** The program the launcher runs (a picked Java folder leads to it). */
  executable: string;
  major?: number;
  version?: string;
  vendor?: string;
  bits32: boolean;
  /** Gone or not working. */
  problem?: "missing" | "broken";
  /** Too old or too new for the instance's game or its mods. */
  fit?: "tooOld" | "tooNew";
  /** The lowest Java the game needs (the highest when this one is too new). */
  need?: number;
  /** The mods raised the lowest Java above the game's own. */
  forMods: boolean;
}

export interface JavaApi {
  /** Instance settings → Java → "Özel": checks a Java the player picked or found. */
  check(instanceId: string, path: string): Promise<JavaCheck>;
  /** The native file picker for a Java program; null when the player closes it. */
  pickExecutable(): Promise<string | null>;
  /** Settings → Java; `rescan` looks for Javas on the computer again. */
  overview(rescan?: boolean): Promise<JavaOverview>;
  /** "Güncelle": updates the launcher runtime as soon as no game runs. */
  update(component: string, platform: string): Promise<void>;
  /** "Kaldır": removes an unused launcher runtime; returns the bytes freed. */
  remove(component: string, platform: string): Promise<number>;
  /** "Otomatik'e geç": returns how many instances changed. */
  switchToAuto(instanceIds: string[]): Promise<number>;
  openFolder(id: string): Promise<void>;
  /** The overview changed (an update started, finished or failed). Returns an unsubscribe. */
  onChange(listener: () => void): () => void;
  /** An update is downloading. Returns an unsubscribe. */
  onProgress(listener: (id: string, percent: number) => void): () => void;
}
