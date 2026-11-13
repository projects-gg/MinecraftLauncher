// Where the stream's stage is now (SQOL §1.1, §4): over the page beside the rail ("page"), small in
// the corner ("corner"), over the whole window ("full"), or not there (null). The room panel docks
// beside it in "page" and hides while it is "full". SQOL-U1 owns the stage and its store
// (`useShareStage((s) => s.mode)`); until that lands this reads the same from the stage's own
// state, so the room panel's branch works alone.

import { stageWatches } from "@/features/share/stage";
import { useShareUi } from "@/features/share/store";
import { useVoiceUi } from "./store";

export type StageMode = "page" | "corner" | "full" | null;

export function useStageMode(): StageMode {
  const watching = useVoiceUi((s) => {
    const share = s.state.share?.on ? s.state.share : null;
    return (
      share !== null &&
      share.surface === "main" &&
      stageWatches(share, s.state.room?.me.id ?? null).length > 0
    );
  });
  const small = useShareUi((s) => s.small);
  const full = useShareUi((s) => s.full);
  if (!watching) return null;
  if (full) return "full";
  return small ? "corner" : "page";
}
