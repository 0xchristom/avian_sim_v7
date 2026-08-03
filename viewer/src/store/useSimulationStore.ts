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
  sick: boolean;
  vitality: number;
  // 6.1: remembered food locations as [x, y, strength] — fading memory dots.
  memory: Array<[number, number, number]>;
}

export interface PredatorSnapshot {
  uid: string;
  pos: [number, number];
  lifetime_remaining_s: number;
  // 6.2: hunt-state machine + dynamic speed scale surfaced for the viewer.
  hunt_state: string;
  speed_level: number;
  meals_eaten: number;
}

export interface ObstacleSnapshot {
  id: number;
  kind: string;
  min: [number, number];
  max: [number, number];
}

export interface SimulationSnapshot {
  frame: number;
  time_us: number;
  light_level: number;
  weather: string;
  weather_intensity: number;
  agents: AgentSnapshot[];
  grains: Array<[number, number]>;
  predators: PredatorSnapshot[];
  obstacles: ObstacleSnapshot[];
  agent_count: number;
  dead_count: number;
}

// 6.1: one entry in the event log panel.
export interface EventLogEntry {
  frame: number;
  event: string;
}

// 6.2: dashboard metrics pushed every ~100 frames by the server.
export interface Metrics {
  frame: number;
  agents: number;
  mean_energy_kj: number;
  mean_hunger: number;
  mean_age_years: number;
  mean_vitality: number;
  flocks: number;
  flocked_agents: number;
  predator_count: number;
  predator_kills: number;
  grains: number;
  spatial_entropy: number;
  forage_rate_g_m2_s: number;
  survival: Array<[number, number]>;
  fsm: Record<string, number>;
}

interface SimStore {
  // Audit 3 Phase 4: double-buffered snapshots. When a new WS snapshot lands,
  // `snapshot` becomes `previousSnapshot` and the new one becomes `snapshot`.
  // `lastReceivedAt`/`currentReceivedAt` (performance.now()) bracket the two
  // arrivals so the renderer can derive an interpolation alpha that trails the
  // real-time clock by one inter-arrival interval (smooth 60fps even when the
  // server drops frames or WS packets arrive in bursts).
  snapshot: SimulationSnapshot | null;
  previousSnapshot: SimulationSnapshot | null;
  lastReceivedAt: number;
  currentReceivedAt: number;
  setSnapshot: (s: SimulationSnapshot) => void;
  eventLog: EventLogEntry[];
  // 6.1: append scenario events (from `{"type":"event_log",...}` WS messages).
  appendEvents: (frame: number, events: unknown[]) => void;
  metrics: Metrics | null;
  setMetrics: (m: Metrics) => void;
}

export const useSimulationStore = create<SimStore>((set) => ({
  snapshot: null,
  previousSnapshot: null,
  lastReceivedAt: 0,
  currentReceivedAt: 0,
  setSnapshot: (s) =>
    set((state) => {
      const now = performance.now();
      return {
        snapshot: s,
        previousSnapshot: state.snapshot,
        lastReceivedAt: state.snapshot ? state.currentReceivedAt : 0,
        currentReceivedAt: now,
      };
    }),
  eventLog: [],
  appendEvents: (frame, events) => {
    if (!events || events.length === 0) return;
    const entries: EventLogEntry[] = events
      .map((raw) => {
        const ev = raw as { event?: string };
        return ev && typeof ev.event === 'string' ? { frame, event: ev.event } : null;
      })
      .filter((e): e is EventLogEntry => e !== null);
    if (entries.length === 0) return;
    set((state) => ({
      // Keep the last 200 entries so the panel never grows unbounded.
      eventLog: [...state.eventLog, ...entries].slice(-200),
    }));
  },
  metrics: null,
  setMetrics: (m) => set({ metrics: m }),
}));
