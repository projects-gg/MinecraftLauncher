// Lines that carry a password, for the UI's own care (the field hides what is typed, the lock
// shows it is sealed). The backend decides what is sealed (pl_cloud_afk::looks_sensitive); this is
// the same list: pl-afk's `SENSITIVE_COMMANDS` (crates/pl-afk/src/console.rs).
const SENSITIVE_COMMANDS: readonly string[] = [
  "/login",
  "/l",
  "/log",
  "/signin",
  "/g",
  "/gir",
  "/giris",
  "/giriş",
  "/girisyap",
  "/girişyap",
  "/register",
  "/reg",
  "/signup",
  "/k",
  "/kayit",
  "/kayıt",
  "/kaydol",
  "/password",
  "/pass",
  "/passwd",
  "/sifre",
  "/şifre",
  "/changepassword",
  "/changepass",
  "/cp",
  "/auth",
  "/authme",
  "/2fa",
  "/2fauth",
];

/** Each character's simple uppercase (`ı` and `i` both `I`); longer expansions stay as they are. */
function fold(text: string): string {
  let out = "";
  for (const char of text) {
    const upper = char.toUpperCase();
    out += [...upper].length === 1 ? upper : char;
  }
  return out;
}

const FOLDED = new Set(SENSITIVE_COMMANDS.map(fold));
const TURKISH_LOWER = new Set(SENSITIVE_COMMANDS.map((name) => name.toLocaleLowerCase("tr")));

/**
 * What a client trims before it reads a line (MCC does): White_Space and
 * Default_Ignorable_Code_Point. A `\` after them still starts its internal command
 * (pl_cloud_afk::without_leading_ignorable, the contract's text rule 4).
 */
const LEADING_IGNORABLE = /^[\p{White_Space}\p{Default_Ignorable_Code_Point}]+/u;

export function withoutLeadingIgnorable(text: string): string {
  return text.replace(LEADING_IGNORABLE, "");
}

/** The first word (up to a space or tab), after the leading characters a client trims. */
export function firstWord(line: string): string {
  return withoutLeadingIgnorable(line).trim().split(/[ \t]/, 1)[0] ?? "";
}

/** The first word is a password command (also a Turkish keyboard's capitals: `/GİRİŞ`). */
export function looksSensitive(line: string): boolean {
  const name = firstWord(line);
  return name !== "" && (FOLDED.has(fold(name)) || TURKISH_LOWER.has(name.toLocaleLowerCase("tr")));
}

/** What a secret row shows instead of its words: `/login ******`, or only the stars. */
export function secretPreview(line: string): string {
  const name = firstWord(line);
  return name.startsWith("/") && name.length <= 32 ? `${name} ******` : "******";
}
