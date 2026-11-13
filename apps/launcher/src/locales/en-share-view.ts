// Watching a stream (SQOL-U1; sqol.md §1.1): the stream tile's bar and menu, its states, "Stream
// Info", zoom, what screen readers hear and the "While Watching" settings. Titles, menu rows,
// buttons and setting names in Title Case; states and sentences in sentence case.
export const enShareView = {
  stage: {
    label: "Streams You're Watching",
  },
  menu: {
    volume: "Stream Volume",
    mute: "Mute Stream",
    quality: "Quality",
    duck: "Lower While Someone Talks",
    fullscreen: "Full Screen",
    exitFullscreen: "Exit Full Screen",
    popout: "Open in a Separate Window",
    popin: "Back to the Launcher",
    onTop: "Always on Top",
    forward: "Bring This Stream Forward",
    sideBySide: "Show Side by Side",
    info: "Stream Info",
  },
  quality: {
    auto: "Automatic",
    high: "High",
    saver: "Data Saver",
  },
  bar: {
    soundOff: "Stream Volume Off",
    more: "Stream Options",
    weak: "Low Quality",
    weakHint: "The connection is weak, so the picture is in low quality.",
  },
  state: {
    hold: "The stream is paused.",
    min: "The shared window is minimized.",
    ended: "The stream ended.",
  },
  info: {
    line: "{{size}} · {{fps}} fps · {{layer}}",
    f: "Full Quality",
    q: "Low Quality",
  },
  zoom: {
    reset: "Reset Zoom",
    label: "Reset Zoom, {{value}}",
  },
  // Said politely to screen readers (not shown).
  say: {
    live: "Live now: {{name}}",
    open: "Stream opened: {{name}}",
    hold: "Stream paused: {{name}}",
    min: "Shared window minimized: {{name}}",
    resume: "Stream back: {{name}}",
    end: "Stream ended: {{name}}",
    soundOn: "Stream sound on.",
    soundOff: "Stream muted.",
    volume: "Stream volume {{value}}",
  },
  chat: {
    label: "Chat",
    unread_one: "Chat, {{count}} unread message",
    unread_other: "Chat, {{count}} unread messages",
  },
  corner: {
    grow: "Make Bigger: {{name}}'s stream",
  },
  own: {
    label: "Your Stream's Preview",
    preview: "Preview",
    close: "Close Preview",
  },
  settings: {
    watching: "While Watching",
    sharing: "While Sharing",
    quality: "Stream Quality",
    duck: "Lower the Stream When Someone Talks",
    duckLevel: "Lower By",
    duckSelf: "Lower When I Talk Too",
    thumbs: "Share a Preview Picture",
  },
};
