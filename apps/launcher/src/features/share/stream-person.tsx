// The sharer's person rows in the stream menu (SQOL-U1, FIX2): given by the launcher's window only
// (imported by host.tsx), so the person menu and the friend list it reads stay out of the "Yayın"
// window's page.
import { usePersonItems } from "@/features/social/person-menu";
import { useFace } from "@/features/voice/store";
import { givePersonRows } from "./stream-menu";

function useSharerRows(person: string) {
  const face = useFace(person);
  return usePersonItems(
    { id: face.id, name: face.name, avatar: face.avatar, avatarUrl: face.avatarUrl },
    {},
  );
}

givePersonRows(useSharerRows);
