// Opus as voice chat sends it (design §6.2): mono, a capped bit rate, silence suppression (DTX)
// and in-band error correction. The sender reads these from the other side's description, so the
// page writes them into every description it receives (the SFU's answer, and its later offers,
// which carry the page's own sending section among the people it receives) before applying it.

export interface OpusTuning {
  /** Bits per second. */
  bitrate: number;
  dtx: boolean;
  fec: boolean;
}

/** `sdp` with every Opus format's parameters set to `tuning` (other parameters and lines kept). */
export function tuneOpus(sdp: string, tuning: OpusTuning): string {
  const newline = sdp.includes("\r\n") ? "\r\n" : "\n";
  const out: string[] = [];
  let section: string[] = [];
  for (const line of sdp.split(newline)) {
    if (line.startsWith("m=")) {
      out.push(...tuneSection(section, tuning));
      section = [];
    }
    section.push(line);
  }
  out.push(...tuneSection(section, tuning));
  return out.join(newline);
}

function tuneSection(lines: string[], tuning: OpusTuning): string[] {
  const payloads = lines
    .map((line) => /^a=rtpmap:(\d+) opus\/48000/i.exec(line)?.[1])
    .filter((found): found is string => found !== undefined);
  const wanted: Record<string, string> = {
    stereo: "0",
    "sprop-stereo": "0",
    maxaveragebitrate: String(Math.round(tuning.bitrate)),
    usedtx: tuning.dtx ? "1" : "0",
    useinbandfec: tuning.fec ? "1" : "0",
  };
  for (const payload of payloads) {
    const prefix = `a=fmtp:${payload} `;
    const at = lines.findIndex((line) => line.startsWith(prefix));
    const params = new Map<string, string>();
    if (at >= 0) {
      for (const pair of (lines[at] ?? "").slice(prefix.length).split(";")) {
        const [key, value = ""] = pair.trim().split("=");
        if (key) params.set(key, value);
      }
    }
    for (const [key, value] of Object.entries(wanted)) params.set(key, value);
    const fmtp = prefix + [...params].map(([key, value]) => `${key}=${value}`).join(";");
    if (at >= 0) {
      lines[at] = fmtp;
    } else {
      const rtpmap = lines.findIndex((line) => line.startsWith(`a=rtpmap:${payload} `));
      lines.splice(rtpmap + 1, 0, fmtp);
    }
  }
  return lines;
}
