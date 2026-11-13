// Where the stream's stage is now (SQOL §1.1, §4): over the page beside the rail ("page"), small in
// the corner ("corner"), over the whole window ("full"), or not there (null). The room panel docks
// beside it in "page" and hides while it is "full". The stage says where it is (SQOL-U1's
// `useShareStage`, which its own tile and an ended stream's tile count too).

import { type StageMode, useShareStage } from "@/features/share/store";

export type { StageMode };

export function useStageMode(): StageMode {
  return useShareStage((s) => s.mode);
}
