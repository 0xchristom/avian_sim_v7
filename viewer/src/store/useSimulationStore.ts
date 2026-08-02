import { create } from 'zustand';

export interface AgentSnapshot {
  uid: string;
  pos: [number, number];
  heading: number;
  vel: [number, number];
  mass_g: number;
  age_years: number;
  energy_kj: number;
  hunger: number;
  fsm_state: string;
  head_offset: [number, number];
  alarm_triggered: boolean;
}

export interface PredatorSnapshot {
  uid: string;
  pos: [number, number];
  lifetime_remaining_s: number;
}

export interface SimulationSnapshot {
  frame: number;
  time_us: number;
  light_level: number;
  agents: AgentSnapshot[];
  grains: Array<[number, number]>;
  predators: PredatorSnapshot[];
  agent_count: number;
  dead_count: number;
}

interface SimStore {
  snapshot: SimulationSnapshot | null;
  setSnapshot: (s: SimulationSnapshot) => void;
}

export const useSimulationStore = create<SimStore>((set) => ({
  snapshot: null,
  setSnapshot: (s) => set({ snapshot: s }),
}));
