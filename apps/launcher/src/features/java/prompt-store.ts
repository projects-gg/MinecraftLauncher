// The Java question before a launch (J1): the launch store opens it when the backend stops a
// launch with `javaMismatch`; the dialog answers by launching again with the choice.
import { create } from "zustand";
import type { JavaPrompt, LaunchOptions } from "@/lib/api";

interface PromptRequest {
  prompt: JavaPrompt;
  /** The stopped launch's options (auto-join, "Devam et"), kept for the next attempt. */
  options?: LaunchOptions;
}

interface JavaPromptState {
  request: PromptRequest | null;
  open: (request: PromptRequest) => void;
  close: () => void;
}

export const useJavaPrompt = create<JavaPromptState>()((set) => ({
  request: null,
  open: (request) => set({ request }),
  close: () => set({ request: null }),
}));
