// CREW-PLUS: crew plan templates and how long a friend has been playing (en-GB). Chips, buttons and
// menu rows in Title Case; sentences as sentences.
export const enCrewPlus = {
  templates: {
    label: "Plan templates",
    builtIn: {
      tonight: { name: "Tonight", note: "Hop on voice" },
      friday: { name: "Friday Night", note: "Everyone come" },
      weekend: { name: "Weekend", note: "Long session" },
    },
    fill: "{{name}}: {{when}}",
    save: "Save as Template",
    saved: "Saved as a template",
    exists: "You already have this template.",
    full: "You can keep up to 5 templates. Delete one to add another.",
    rename: "Rename",
    name: "Template name",
    renameSave: "Save",
    renameCancel: "Cancel",
    remove: "Delete",
    removed: "Template deleted",
    undo: "Undo",
  },
  privacy: {
    playTime: "Show My Play Time",
    playTimeNeedsSeen: "Shows only while your last seen is on.",
  },
  played: {
    minutes: "{{count}} min",
    hours: "{{count}} h",
    spoken: {
      minutes_one: "playing for {{count}} minute",
      minutes_other: "playing for {{count}} minutes",
      hours_one: "playing for {{count}} hour",
      hours_other: "playing for {{count}} hours",
    },
  },
};
