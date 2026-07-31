import { create } from 'zustand';

interface SimulationSnapshot {
  frame: number;
  time_us: number;
  agents: Array<{
    uid: string;
    pos: [number, number];
    heading: number;
    vel: [number, number];
    mass_g: number;
    age_years: number;
    energy_kj: number;
    hunger: number;
    fsm_state: string;
  }>;
}

interface SimStore {
  snapshot: SimulationSnapshot | null;
  isPlaying: boolean;
  playbackSpeed: number;
  setSnapshot: (s: SimulationSnapshot) => void;
  togglePlay: () => void;
}

export const useSimulationStore = create<SimStore>((set) => ({
  snapshot: null,
  isPlaying: true,
  playbackSpeed: 1.0,
  setSnapshot: (s) => set({ snapshot: s }),
  togglePlay: () => set((state) => ({ isPlaying: !state.isPlaying })),
}));
