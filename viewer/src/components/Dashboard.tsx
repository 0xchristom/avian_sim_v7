import React from 'react';

interface AgentData {
  uid: string;
  age_years: number;
  mass_g: number;
  fsm_state: string;
  energy_kj: number;
  hunger: number;
  alarm_triggered: boolean;
}

interface PredatorData {
  uid: string;
  pos: [number, number];
  lifetime_remaining_s: number;
}

interface DashboardProps {
  agents: AgentData[];
  selectedUids: string[];
  lightLevel?: number;
  deadCount?: number;
  predatorCount?: number;
  predators?: PredatorData[];
}

export const Dashboard: React.FC<DashboardProps> = ({ agents, selectedUids, lightLevel, deadCount, predatorCount, predators = [] }) => {
  const displayedAgents = selectedUids.length > 0 ? agents.filter(a => selectedUids.includes(a.uid)) : agents;
  const selectedPredators = selectedUids.length > 0 ? predators.filter(p => selectedUids.includes(p.uid)) : [];

  return (
    <div>
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: '6px', marginBottom: '12px', fontSize: '11px', textAlign: 'center' }}>
        <div style={{ background: '#1a1c25', padding: '6px', borderRadius: '4px' }}>
          <div style={{ color: '#888' }}>ŚWIATŁO</div>
          <div style={{ color: '#ffcc00' }}>{(lightLevel ?? 1) * 100}%</div>
        </div>
        <div style={{ background: '#1a1c25', padding: '6px', borderRadius: '4px' }}>
          <div style={{ color: '#888' }}>DRAPIEŻNIKI</div>
          <div style={{ color: '#ff2222' }}>{predatorCount ?? 0}</div>
        </div>
        <div style={{ background: '#1a1c25', padding: '6px', borderRadius: '4px' }}>
          <div style={{ color: '#888' }}>ZGONY</div>
          <div style={{ color: '#ff3366' }}>{deadCount ?? 0}</div>
        </div>
      </div>

      <h3>ZAZNACZENI AGENTSI ({displayedAgents.length + selectedPredators.length})</h3>
      {selectedUids.length > 0 && <p style={{ color: '#888', fontSize: '12px' }}>Tryb selekcji. Zaznacz obszar lub kliknij pojedynczo. ESC, aby anulować.</p>}
      {selectedPredators.map(selectedPredator => (
        <div key={selectedPredator.uid} className="agent-card" style={{ border: '1px solid #ff2222' }}>
          <h4 style={{ color: '#ff2222' }}>DRAPIEŻNIK: {selectedPredator.uid} <span style={{color:'#888', fontSize:'10px', fontWeight:'normal'}}>[LIVE]</span></h4>
          <div className="stat-row"><span>POZYCJA</span><span style={{color:'#00ffcc'}}>({selectedPredator.pos[0].toFixed(1)}, {selectedPredator.pos[1].toFixed(1)})</span></div>
          <div className="stat-row"><span>ZANIK W</span><span style={{color:'#ff2222'}}>{selectedPredator.lifetime_remaining_s.toFixed(1)} s</span></div>
          <div className="bar-bg"><div className="bar-fill" style={{ width: `${Math.min(100, Math.max(0, selectedPredator.lifetime_remaining_s / 15 * 100))}%`, background: '#ff2222' }}></div></div>
        </div>
      ))}
      {displayedAgents.length === 0 && selectedPredators.length === 0 && <p style={{ color: '#888' }}>Oczekiwanie na dane z silnika Rust...</p>}
      {displayedAgents.map(a => (
        <div key={a.uid} className="agent-card">
          <h4>UID: {a.uid} <span style={{color:'#888', fontSize:'10px', fontWeight:'normal'}}>[LIVE]</span> {a.alarm_triggered && <span style={{color:'#ff2222', fontSize:'10px', fontWeight:'bold'}}>🚨 ALARM</span>}</h4>
          <div className="stat-row"><span>WIEK</span><span style={{color:'#00ffcc'}}>{a.age_years.toFixed(1)} lat</span></div>
          <div className="stat-row"><span>WAGA</span><span style={{color:'#ff3366'}}>{a.mass_g.toFixed(0)} g</span></div>
          <div className="stat-row"><span>FSM STATE</span><span style={{color:'#00ffcc'}}>{a.fsm_state}</span></div>
          <div className="stat-row"><span>ENERGIA (E)</span><span>{a.energy_kj.toFixed(1)} kJ</span></div>
          <div className="bar-bg"><div className="bar-fill" style={{ width: `${Math.min(100, a.energy_kj)}%`, background: '#00ff00' }}></div></div>
          <div className="stat-row"><span>GŁÓD (H)</span><span>{(a.hunger * 100).toFixed(0)}%</span></div>
          <div className="bar-bg"><div className="bar-fill" style={{ width: `${a.hunger * 100}%`, background: '#ffaa00' }}></div></div>
        </div>
      ))}
    </div>
  );
};
