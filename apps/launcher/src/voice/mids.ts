// Which person each receiver plays (§7.4: every SFU offer names its new receivers, mid → person).
// A person who comes back gets a new receiver; the old one is dead from then on (the room closes
// dead ones in batches), and a mid the SFU hands out again belongs to whoever it names last.

export class MidMap {
  private readonly byMid = new Map<string, { person: string; order: number }>();
  private readonly newest = new Map<string, string>();
  private order = 0;

  /** An offer's receivers; returns the mids whose person changed (new ones included). */
  assign(mids: { [mid in string]: string }): string[] {
    const changed: string[] = [];
    for (const [mid, person] of Object.entries(mids)) {
      if (typeof person !== "string" || !person) continue;
      const before = this.byMid.get(mid);
      this.order += 1;
      this.byMid.set(mid, { person, order: this.order });
      this.newest.set(person, mid);
      if (before?.person !== person) changed.push(mid);
    }
    return changed;
  }

  personOf(mid: string): string | undefined {
    return this.byMid.get(mid)?.person;
  }

  /** Whether `mid` is its person's newest receiver (an older one is dead). */
  current(mid: string): boolean {
    const person = this.byMid.get(mid)?.person;
    return person !== undefined && this.newest.get(person) === mid;
  }

  clear(): void {
    this.byMid.clear();
    this.newest.clear();
    this.order = 0;
  }
}
