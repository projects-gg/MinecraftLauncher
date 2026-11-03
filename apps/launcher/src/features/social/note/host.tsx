// What SOC-NOTE runs in the background, mounted once with SOC-NEXT's (next/host.tsx): the notes'
// one clock (it moves at the soonest end on screen), "Not ekle" and "Sabitler dolu".
import { NoteDialog } from "./note-dialog";
import { useNoteClockDriver } from "./notes";
import { PinReplaceDialog } from "./pins";

export function SocNoteHost() {
  useNoteClockDriver();
  return (
    <>
      <NoteDialog />
      <PinReplaceDialog />
    </>
  );
}
