import { describe, expect, it } from "vitest";
import { tuneOpus } from "./sdp";

const ANSWER = [
  "v=0",
  "m=audio 9 UDP/TLS/RTP/SAVPF 111 63 9",
  "a=rtpmap:111 opus/48000/2",
  "a=rtcp-fb:111 transport-cc",
  "a=fmtp:111 minptime=10;useinbandfec=1",
  "a=rtpmap:63 red/48000/2",
  "a=fmtp:63 111/111",
  "a=rtpmap:9 G722/8000",
  "",
].join("\r\n");

describe("tuneOpus", () => {
  it("sets Opus's parameters and keeps the rest", () => {
    const tuned = tuneOpus(ANSWER, { bitrate: 24000, dtx: true, fec: true });
    expect(tuned).toContain(
      "a=fmtp:111 minptime=10;useinbandfec=1;stereo=0;sprop-stereo=0;maxaveragebitrate=24000;usedtx=1\r\n",
    );
    expect(tuned).toContain("a=fmtp:63 111/111\r\n");
    expect(tuned.split("\r\n")).toHaveLength(ANSWER.split("\r\n").length);
  });

  it("overwrites earlier values and is stable", () => {
    const once = tuneOpus(ANSWER, { bitrate: 16000, dtx: false, fec: false });
    expect(once).toContain("useinbandfec=0");
    expect(once).toContain("usedtx=0");
    expect(tuneOpus(once, { bitrate: 16000, dtx: false, fec: false })).toBe(once);
  });

  it("adds the format line when there is none", () => {
    const bare = "m=audio 9 RTP/AVP 109\na=rtpmap:109 OPUS/48000/2\na=sendrecv";
    expect(tuneOpus(bare, { bitrate: 24000, dtx: true, fec: true }).split("\n")).toEqual([
      "m=audio 9 RTP/AVP 109",
      "a=rtpmap:109 OPUS/48000/2",
      "a=fmtp:109 stereo=0;sprop-stereo=0;maxaveragebitrate=24000;usedtx=1;useinbandfec=1",
      "a=sendrecv",
    ]);
  });

  it("leaves a description without Opus alone", () => {
    const g722 = "m=audio 9 RTP/AVP 9\r\na=rtpmap:9 G722/8000";
    expect(tuneOpus(g722, { bitrate: 24000, dtx: true, fec: true })).toBe(g722);
  });

  // The SFU's offer after a pull: the page's own sending section first (the SFU receives it), then
  // one section per person received. The encoder reads its section's parameters, wherever it is.
  const SFU_OFFER = [
    "v=0",
    "o=- 1 2 IN IP4 127.0.0.1",
    "s=-",
    "t=0 0",
    "a=group:BUNDLE 0 1 2",
    "m=audio 9 UDP/TLS/RTP/SAVPF 111",
    "a=mid:0",
    "a=recvonly",
    "a=rtpmap:111 opus/48000/2",
    "a=fmtp:111 minptime=10;useinbandfec=1",
    "m=audio 9 UDP/TLS/RTP/SAVPF 111",
    "a=mid:1",
    "a=sendonly",
    "a=rtpmap:111 opus/48000/2",
    "a=fmtp:111 minptime=10;useinbandfec=1",
    "m=audio 9 UDP/TLS/RTP/SAVPF 100",
    "a=mid:2",
    "a=sendonly",
    "a=rtpmap:100 opus/48000/2",
    "",
  ].join("\r\n");

  it("tunes every section of an SFU offer and keeps its order", () => {
    const tuned = tuneOpus(SFU_OFFER, { bitrate: 32000, dtx: true, fec: true });
    const lines = tuned.split("\r\n");
    const fmtp = lines.filter((line) => line.startsWith("a=fmtp:"));
    expect(fmtp).toEqual([
      "a=fmtp:111 minptime=10;useinbandfec=1;stereo=0;sprop-stereo=0;maxaveragebitrate=32000;usedtx=1",
      "a=fmtp:111 minptime=10;useinbandfec=1;stereo=0;sprop-stereo=0;maxaveragebitrate=32000;usedtx=1",
      "a=fmtp:100 stereo=0;sprop-stereo=0;maxaveragebitrate=32000;usedtx=1;useinbandfec=1",
    ]);
    // The added line sits under its own section's rtpmap, sections untouched otherwise.
    expect(
      lines.indexOf(
        "a=fmtp:100 stereo=0;sprop-stereo=0;maxaveragebitrate=32000;usedtx=1;useinbandfec=1",
      ),
    ).toBe(lines.indexOf("a=rtpmap:100 opus/48000/2") + 1);
    expect(lines.filter((line) => line.startsWith("m=")).length).toBe(3);
    expect(lines.filter((line) => line.startsWith("a=mid:"))).toEqual([
      "a=mid:0",
      "a=mid:1",
      "a=mid:2",
    ]);
    expect(lines).toHaveLength(SFU_OFFER.split("\r\n").length + 1);
    expect(tuneOpus(tuned, { bitrate: 32000, dtx: true, fec: true })).toBe(tuned);
  });

  it("leaves the session part alone", () => {
    const tuned = tuneOpus(SFU_OFFER, { bitrate: 24000, dtx: true, fec: true });
    expect(
      tuned.startsWith(
        "v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\na=group:BUNDLE 0 1 2\r\n",
      ),
    ).toBe(true);
  });
});
