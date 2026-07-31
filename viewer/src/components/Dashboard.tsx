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
    <div style={{ width: '300px', background: '#111', padding: '10px', color: '#fff' }}>
      <h3>Agents ({agents.length})</h3>
      {agents.map(a => (
        <div key={a.uid} style={{ border: '1px solid #444', margin: '5px', padding: '5px' }}>
          <div>UID: {a.uid}</div>
          <div>Age: {a.age_years.toFixed(1)}y</div>
          <div>Mass: {a.mass_g.toFixed(0)}g</div>
          <div>State: {a.fsm_state}</div>
          <div>Energy: {a.energy_kj.toFixed(1)} kJ</div>
          <progress value={a.energy_kj} max="50" style={{ width: '100%' }} />
          <div>Hunger: {(a.hunger * 100).toFixed(0)}%</div>
          <progress value={a.hunger} max="1" style={{ width: '100%' }} />
        </div>
      ))}
    </div>
  );
};
