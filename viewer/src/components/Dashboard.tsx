import React from 'react';

interface AgentData {
  uid: string;
  age_years: number;
  mass_g: number;
  fsm_state: string;
  energy_kj: number;
  hunger: number;
}

export const Dashboard: React.FC<{ agents: AgentData[] }> = ({ agents }) => {
  return (
    <div className="dashboard">
      <h3>ZAZNACZENI AGENTSI ({agents.length})</h3>
      {agents.length === 0 && <p style={{ color: '#888' }}>Oczekiwanie na dane z silnika Rust...</p>}
      {agents.map(a => (
        <div key={a.uid} className="agent-card">
          <h4>UID: {a.uid} <span style={{color:'#888', fontSize:'10px', fontWeight:'normal'}}>[LIVE]</span></h4>
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