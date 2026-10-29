// Discord gives an account without a picture of its own a stand-in: its logo on a colour. That is
// Discord's mark, not the player's face, so the launcher shows the player's letter instead, as for
// anyone without a picture (social-face.tsx).

const DISCORD_STAND_IN = /^https:\/\/cdn\.discordapp\.com\/embed\/avatars\//;

/** The photo to show for a picture address: none for Discord's stand-in. */
export function ownPhoto(url: string | undefined): string | undefined {
  return url && !DISCORD_STAND_IN.test(url) ? url : undefined;
}
