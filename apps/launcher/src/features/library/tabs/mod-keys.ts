// React keys of the mods tab's rows. A row keeps its key when its mod is switched off or on (the
// file is renamed to or from `.disabled`), so the row stays where it is instead of leaving and
// coming back. A folder can hold both `x.jar` and `x.jar.disabled` (the player copied a mod in
// twice; the backend lists both): those rows take their full file names, one key per row.

const DISABLED = /\.disabled$/;

/** The file name without `.disabled`. */
export function baseFileName(fileName: string): string {
  return fileName.replace(DISABLED, "");
}

/** Each file's row key: its base name, or its full name when another file shares that base. */
export function modRowKeys(fileNames: readonly string[]): Map<string, string> {
  const bases = new Map<string, number>();
  for (const name of fileNames) {
    const base = baseFileName(name);
    bases.set(base, (bases.get(base) ?? 0) + 1);
  }
  const keys = new Map<string, string>();
  const used = new Set<string>();
  for (const name of fileNames) {
    if (keys.has(name)) continue;
    const base = baseFileName(name);
    let key = (bases.get(base) ?? 0) > 1 ? name : base;
    // Names like `a.jar.disabled.disabled` could still meet another row's key: number them.
    for (let n = 2; used.has(key); n++) key = `${name}#${n}`;
    used.add(key);
    keys.set(name, key);
  }
  return keys;
}
