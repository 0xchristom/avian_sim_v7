import React, { useEffect, useRef } from 'react';
import { WebGLRenderer } from './renderer/WebGLRenderer';
import { Dashboard } from './components/Dashboard';
import { useSimulationStore } from './store/useSimulationStore';

const App: React.FC = () => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const rendererRef = useRef<WebGLRenderer | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const snapshot = useSimulationStore((state) => state.snapshot);
  const setSnapshot = useSimulationStore((state) => state.setSnapshot);

  useEffect(() => {
    if (canvasRef.current) {
      rendererRef.current = new WebGLRenderer(canvasRef.current);
    }
    
    const ws = new WebSocket('ws://127.0.0.1:8080');
    wsRef.current = ws;
    ws.onopen = () => console.log("✅ Połączono z serwerem Rust!");
    ws.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        setSnapshot(data);
      } catch (e) {
        console.error("Błąd parsowania JSON:", e);
      }
    };

    return () => ws.close();
  }, [setSnapshot]);

  useEffect(() => {
    if (rendererRef.current && snapshot) {
      rendererRef.current.render(snapshot);
    }
  }, [snapshot]);

  const handleCanvasClick = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (!canvasRef.current || !wsRef.current) return;
    const rect = canvasRef.current.getBoundingClientRect();
    const x = ((e.clientX - rect.left) / rect.width) * 32;
    const y = ((e.clientY - rect.top) / rect.height) * 21;
    wsRef.current.send(`spawn_grain,${x},${y}`);
  };

  return (
    <div style={{ display: 'flex', width: '100vw', height: '100vh', backgroundColor: '#0a0b10', color: '#d4d4d4', fontFamily: 'Courier New' }}>
      <div className="sidebar" style={{ width: '400px', height: '100vh', overflowY: 'auto', background: '#14161f', borderRight: '2px solid #ff6c0c', boxSizing: 'border-box', display: 'flex', flexDirection: 'column' }}>
        <div style={{ padding: '15px', borderBottom: '2px solid #ff6c0c', textAlign: 'center' }}>
          <h2 style={{ margin: 0, color: '#ff6c0c' }}>AVIAN SIM v7.0 PRO</h2>
          <p style={{ margin: '5px 0 0 0', fontSize: '12px', color: '#888' }}>Zintegrowany Silnik Rust + WebGL</p>
        </div>
        <div style={{ padding: '15px', flex: 1 }}>
          <Dashboard agents={snapshot?.agents || []} />
        </div>
      </div>
      
      <main style={{ flex: 1, display: 'flex', justifyContent: 'center', alignItems: 'center', padding: '20px' }}>
        <canvas 
          ref={canvasRef} 
          width={800} 
          height={533} 
          onClick={handleCanvasClick}
          style={{ background: '#1a1c25', border: '1px solid #333', boxShadow: '0 0 20px rgba(0,255,204,0.1)', cursor: 'crosshair' }} 
        />
      </main>
    </div>
  );
};

export default App;