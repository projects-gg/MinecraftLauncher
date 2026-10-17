// The name a new instance gets when the player leaves the name empty.

const same = (a: string, b: string) =>
  a.trim().toLocaleLowerCase("tr") === b.trim().toLocaleLowerCase("tr");

/**
 * `base` ("Fabric 1.21.4", "Minecraft 1.21.4") when no instance has that name yet, else the
 * first free numbered copy ("Fabric 1.21.4 (2)").
 */
export function defaultInstanceName(base: string, taken: readonly string[]): string {
  const name = base.trim();
  const free = (candidate: string) => !taken.some((other) => same(other, candidate));
  if (free(name)) return name;
  for (let n = 2; ; n++) {
    const candidate = `${name} (${n})`;
    if (free(candidate)) return candidate;
  }
}
