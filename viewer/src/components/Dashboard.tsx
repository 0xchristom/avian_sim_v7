import React from 'react';

interface AgentData {
  uid: string;
  age_years: number;
  mass_g: number;
  fsm_state: string;
  energy_kj: number;
  hunger: number;
  alarm_triggered: boolean;
  sick: boolean;
}

interface PredatorData {
  uid: string;
  pos: [number, number];
  lifetime_remaining_s: number;
  hunt_state: string;
  speed_level: number;
  meals_eaten: number;
}

// 6.2: dashboard metrics pushed every ~100 frames by the server.
interface Metrics {
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

interface DashboardProps {
  agents: AgentData[];
  selectedUids: string[];
  lightLevel?: number;
  deadCount?: number;
  predatorCount?: number;
  predators?: PredatorData[];
  metrics?: Metrics | null;
  onSelectUid?: (uid: string) => void;
}

// FSM → accent color for the compact status cards.
const FSM_COLORS: Record<string, string> = {
  Foraging: '#00ff00',
  Fleeing: '#ff2222',
  Preening: '#00e5ff',
  Resting: '#aaaaaa',
  NightRest: '#4488ff',
  Wandering: '#ffcc00',
  CriticalEnergy: '#ff6600',
};

const fsmColor = (state: string) => FSM_COLORS[state] ?? '#888';

// Compact mini-card: several pigeons per row so statuses scan quickly.
const AgentMiniCard: React.FC<{ a: AgentData; selected: boolean; onSelectUid?: (uid: string) => void }> = ({ a, selected, onSelectUid }) => {
  const color = fsmColor(a.fsm_state);
  return (
    <div
      onClick={() => onSelectUid?.(a.uid)}
      style={{
        background: '#1a1c25',
        border: `1px solid ${selected ? '#ff6c0c' : '#333'}`,
        borderLeft: `3px solid ${color}`,
        borderRadius: '4px',
        padding: '5px 7px',
        cursor: 'pointer',
        display: 'flex',
        flexDirection: 'column',
        gap: '3px',
        fontSize: '10px',
        minWidth: 0,
      }}
      title={`${a.uid} — ${a.fsm_state} · E ${a.energy_kj.toFixed(1)} kJ · H ${(a.hunger * 100).toFixed(0)}%`}
    >
      <div style={{ display: 'flex', justifyContent: 'space-between', gap: '4px', whiteSpace: 'nowrap' }}>
        <span style={{ color: '#00ffcc', overflow: 'hidden', textOverflow: 'ellipsis' }}>{a.uid.replace(/^A(\d{4})/, '$1')}</span>
        <span style={{ color, overflow: 'hidden', textOverflow: 'ellipsis' }}>{a.fsm_state}</span>
      </div>
      <div className="stat-row" style={{ marginBottom: 0 }}>
        <span style={{ color: '#888' }}>E</span>
        <span>{a.energy_kj.toFixed(0)} kJ</span>
      </div>
      <div className="bar-bg" style={{ height: '3px', marginBottom: 0 }}><div className="bar-fill" style={{ width: `${Math.min(100, Math.max(0, a.energy_kj))}%`, background: '#00ff00' }}></div></div>
      <div className="stat-row" style={{ marginBottom: 0 }}>
        <span style={{ color: '#888' }}>H</span>
        <span>{(a.hunger * 100).toFixed(0)}%</span>
      </div>
      <div className="bar-bg" style={{ height: '3px', marginBottom: 0 }}><div className="bar-fill" style={{ width: `${Math.min(100, a.hunger * 100)}%`, background: '#ffaa00' }}></div></div>
      <div style={{ display: 'flex', justifyContent: 'space-between', color: '#777' }}>
        <span>{a.age_years.toFixed(1)}y</span>
        <span>{a.mass_g.toFixed(0)}g</span>
        {a.sick && <span style={{ color: '#ff2222' }}>🦠</span>}
        {a.alarm_triggered && <span style={{ color: '#ff2222' }}>🚨</span>}
      </div>
    </div>
  );
};

export const Dashboard: React.FC<DashboardProps> = ({ agents, selectedUids, lightLevel, deadCount, predatorCount, predators = [], metrics, onSelectUid }) => {
  const displayedAgents = selectedUids.length > 0 ? agents.filter(a => selectedUids.includes(a.uid)) : agents;

  return (
    <div>
      {/* Summary strip */}
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: '6px', marginBottom: '10px', fontSize: '11px', textAlign: 'center' }}>
        <div style={{ background: '#1a1c25', padding: '5px', borderRadius: '4px' }}>
          <div style={{ color: '#888' }}>LIGHT</div>
          <div style={{ color: '#ffcc00' }}>{(lightLevel ?? 1) * 100}%</div>
        </div>
        <div style={{ background: '#1a1c25', padding: '5px', borderRadius: '4px' }}>
          <div style={{ color: '#888' }}>PREDATORS</div>
          <div style={{ color: '#ff2222' }}>{predatorCount ?? 0}</div>
        </div>
        <div style={{ background: '#1a1c25', padding: '5px', borderRadius: '4px' }}>
          <div style={{ color: '#888' }}>DEATHS</div>
          <div style={{ color: '#ff3366' }}>{deadCount ?? 0}</div>
        </div>
      </div>

      {metrics && (
        <div style={{ border: '1px solid #2a6f5c', borderRadius: '4px', padding: '6px 8px', marginBottom: '10px', background: '#10121a' }}>
          <div style={{ margin: '0 0 5px 0', fontSize: '12px', color: '#00ffcc' }}>METRICS (frame #{metrics.frame})</div>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: '2px 10px', fontSize: '10px' }}>
            <div className="stat-row" style={{ marginBottom: 0 }}><span>E AVG</span><span style={{ color: '#00ff00' }}>{metrics.mean_energy_kj.toFixed(1)} kJ</span></div>
            <div className="stat-row" style={{ marginBottom: 0 }}><span>H AVG</span><span style={{ color: '#ffaa00' }}>{(metrics.mean_hunger * 100).toFixed(0)}%</span></div>
            <div className="stat-row" style={{ marginBottom: 0 }}><span>AGE AVG</span><span style={{ color: '#00ffcc' }}>{metrics.mean_age_years.toFixed(1)}y</span></div>
            <div className="stat-row" style={{ marginBottom: 0 }}><span>VITALITY</span><span style={{ color: '#ff9966' }}>{metrics.mean_vitality.toFixed(2)}</span></div>
            <div className="stat-row" style={{ marginBottom: 0 }}><span>FLOCKS</span><span style={{ color: '#00ffcc' }}>{metrics.flocks.toFixed(0)} ({metrics.flocked_agents.toFixed(0)})</span></div>
            <div className="stat-row" style={{ marginBottom: 0 }}><span>ENTROPY</span><span style={{ color: '#ffcc00' }}>{metrics.spatial_entropy.toFixed(2)}</span></div>
            <div className="stat-row" style={{ marginBottom: 0 }}><span>GRAINS</span><span style={{ color: '#00ffcc' }}>{metrics.grains.toFixed(0)}</span></div>
            <div className="stat-row" style={{ marginBottom: 0 }}><span>CAPTURES</span><span style={{ color: '#ff2222' }}>{metrics.predator_kills.toFixed(0)}</span></div>
            <div className="stat-row" style={{ marginBottom: 0 }}><span>FORAGE/m²/s</span><span style={{ color: '#ffcc00' }}>{metrics.forage_rate_g_m2_s.toFixed(3)}</span></div>
          </div>
        </div>
      )}

      <h3 style={{ fontSize: '12px' }}>PIGEONS ({displayedAgents.length})</h3>

      {displayedAgents.length === 0 && predators.length === 0 && <p style={{ color: '#888', fontSize: '11px' }}>Waiting for data from the Rust engine...</p>}

      {/* Predators: full-width cards (few of them, more detail). */}
      {predators.map(p => (
        <div key={p.uid} className="agent-card" style={{ border: '1px solid #ff2222', padding: '6px 8px', marginBottom: '6px' }}>
          <div style={{ color: '#ff2222', fontSize: '11px', marginBottom: '4px' }}>🦅 {p.uid}</div>
          <div className="stat-row" style={{ marginBottom: 0 }}><span>HUNT</span><span style={{ color: '#ffcc00' }}>{p.hunt_state}</span></div>
          <div className="stat-row" style={{ marginBottom: 0 }}><span>SPEED</span><span style={{ color: '#00ffcc' }}>{p.speed_level}/5</span></div>
          <div className="stat-row" style={{ marginBottom: 0 }}><span>MEALS</span><span style={{ color: '#ff9966' }}>{p.meals_eaten}/3</span></div>
          <div className="stat-row" style={{ marginBottom: 0 }}><span>LIFETIME</span><span style={{ color: '#ff2222' }}>{p.lifetime_remaining_s.toFixed(1)}s</span></div>
        </div>
      ))}

      {/* Pigeons: compact grid — several per row. */}
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(2, 1fr)', gap: '5px' }}>
        {displayedAgents.map(a => (
          <AgentMiniCard key={a.uid} a={a} selected={selectedUids.includes(a.uid)} onSelectUid={onSelectUid} />
        ))}
      </div>
    </div>
  );
};
