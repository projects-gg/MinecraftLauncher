// UI-facing shapes of the AFK settings sheet. The generated bindings mark the fields of
// `#[serde(default)]` structs optional and every f64 nullable (serde writes NaN as null); the
// backend always sends complete pages with finite numbers, so the sheet works with complete types
// and only the Tauri adapter converts.
import type * as B from "@/bindings";

/** Every field present and non-null, all the way down (unions stay unions). */
type Complete<T> = T extends readonly (infer U)[]
  ? Complete<U>[]
  : T extends object
    ? { [K in keyof T]-?: Complete<NonNullable<T[K]>> }
    : T;

export type SettingsPage = Complete<B.SettingsPage>;
export type LoginCommandLine = Complete<B.LoginCommandLine>;
export type ScheduledTask = Complete<B.ScheduledTask>;
export type AutoRespondRule = Complete<B.AutoRespondRule>;
/** `unknownVersion` really is optional: the stored version the list does not know, if any. */
export type AfkSettingsView = Complete<Omit<B.AfkSettingsView, "unknownVersion">> & {
  unknownVersion: string | null;
};
export type AfkSettingsSaveResult = Complete<B.AfkSettingsSaveResult>;
export type AfkSettings = Complete<B.AfkSettings>;
export type ChatLogFilter = B.ChatLogFilter;
export type ValidationIssue = B.ValidationIssue;
export type PatternProblem = B.PatternProblem;
export type VersionOption = B.VersionOption;
