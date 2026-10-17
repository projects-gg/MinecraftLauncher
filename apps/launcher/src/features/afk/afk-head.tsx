// An AFK account's face, drawn from what the launcher already has: the cached head of the game
// account with the same name, else the name's initial. No avatar service is asked: a picture
// fetched by name would hand every AFK name and the player's address to a third party
// (docs/security.md §1).
import { PlayerHead } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { sameName } from "@/features/accounts/afk-match";
import { api } from "@/lib/api";

export function AfkHead({
  nickname,
  size,
  className,
}: {
  nickname: string;
  size: number;
  className?: string;
}) {
  // The account list every account surface shares (its cache, its refreshes).
  const { data: accounts } = useQuery({ queryKey: ["accounts"], queryFn: () => api.accounts() });
  const head = accounts?.find((a) => sameName(a.username, nickname))?.headUrl;
  return <PlayerHead name={nickname} src={head} size={size} className={className} />;
}
