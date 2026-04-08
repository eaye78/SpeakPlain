import { create } from "zustand";

interface HistoryItem {
  id: number;
  text: string;
  created_at: string;
  duration_sec: number;
  confidence: number;
}

interface AppState {
  isRecording: boolean;
  isProcessing: boolean;
  history: HistoryItem[];
  currentSkinId: string;
  customSkins: Array<{ id: string; name: string; description?: string }>;
  config: {
    hotkey_vk: number;
    hotkey_name: string;
    audio_device: string | null;
    use_gpu: boolean;
    remove_fillers: boolean;
    capitalize_sentences: boolean;
    optimize_spacing: boolean;
    restore_clipboard: boolean;
    sound_feedback: boolean;
    auto_start: boolean;
    skin_id: string;
  };
  
  // Actions
  setRecording: (value: boolean) => void;
  setProcessing: (value: boolean) => void;
  setHistory: (history: HistoryItem[]) => void;
  addHistoryItem: (text: string) => void;
  deleteHistoryItem: (id: number) => void;
  clearHistory: () => void;
  updateConfig: (config: Partial<AppState["config"]>) => void;
  setCurrentSkinId: (skinId: string) => void;
  setCustomSkins: (skins: Array<{ id: string; name: string; description?: string }>) => void;
}

export const useAppStore = create<AppState>((set) => ({
  isRecording: false,
  isProcessing: false,
  history: [],
  currentSkinId: "classic",
  customSkins: [],
  config: {
    hotkey_vk: 0x71, // F2
    hotkey_name: "F2",
    audio_device: null,
    use_gpu: true,
    remove_fillers: true,
    capitalize_sentences: true,
    optimize_spacing: true,
    restore_clipboard: true,
    sound_feedback: true,
    auto_start: false,
    skin_id: "classic",
  },

  setRecording: (value) => set({ isRecording: value }),
  setProcessing: (value) => set({ isProcessing: value }),
  setHistory: (history) => set({ history }),
  
  addHistoryItem: (text) =>
    set((state) => ({
      history: [
        {
          id: Date.now(),
          text,
          created_at: new Date().toISOString(),
          duration_sec: 0,
          confidence: 0,
        },
        ...state.history,
      ].slice(0, 50),
    })),
    
  deleteHistoryItem: (id) =>
    set((state) => ({
      history: state.history.filter((item) => item.id !== id),
    })),
    
  clearHistory: () => set({ history: [] }),
    
  updateConfig: (newConfig) =>
    set((state) => ({
      config: { ...state.config, ...newConfig },
    })),
    
  setCurrentSkinId: (skinId) =>
    set({ currentSkinId: skinId }),
    
  setCustomSkins: (skins) =>
    set({ customSkins: skins }),
}));
