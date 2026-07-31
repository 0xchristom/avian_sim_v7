import React, { useEffect, useRef } from 'react';
import { WebGLRenderer } from './renderer/WebGLRenderer';
import { Dashboard } from './components/Dashboard';
import { useSimulationStore } from './store/useSimulationStore';

const App: React.FC = () => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const rendererRef = useRef<WebGLRenderer | null>(null);
  const snapshot = useSimulationStore((state) => state.snapshot);
  const setSnapshot = useSimulationStore((state) => state.setSnapshot);

  useEffect(() => {
    if (canvasRef.current) {
      rendererRef.current = new WebGLRenderer(canvasRef.current);
    }
    
    const ws = new WebSocket('ws://127.0.0.1:8080');
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

  return (
    <div style={{ display: 'flex', width: '100vw', height: '100vh', backgroundColor: '#0a0b10', color: '#d4d4d4', fontFamily: 'Courier New' }}>
      <div className="sidebar" style={{ width: '400px', height: '100vh', overflowY: 'auto', background: '#14161f', borderRight: '2px solid #ff6c0c', boxSizing: 'border-box', display: 'flex', flexDirection: 'column' }}>
        <div style={{ padding: '15px', borderBottom: '2px solid #ff6c0c', textAlign: 'center' }}>
          <h2 style={{ margin: 0, color: '#ff6c0c' }}>AVIAN SIM v7.0 PRO</h2>
          <p style={{ margin: '5px 0 0 0', fontSize: '12px', color: '#888' }}>Rust Core + WebGL Viewer</p>
        </div>
        
        <div className="toolbar" style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: '15px', padding: '15px', borderBottom: '1px solid #333' }}>
          <button className="tool-btn active" data-tooltip="Ziarno (Rzucanie)" style={{ aspectRatio: 1, background: '#ff6c0c', border: '1px solid #ff6c0c', cursor: 'pointer', display: 'flex', alignItems: 'center', justifyContent: 'center', borderRadius: '4px' }}>
            <svg viewBox="0 0 24 24" style={{ stroke: '#000', width: '24px', height: '24px', fill: 'none', strokeWidth: 2 }}><circle cx="12" cy="12" r="3"></circle><path d="M12 3v3M12 18v3M3 12h3M18 12h3M5.6 5.6l2.1 2.1M16.3 16.3l2.1 2.1M5.6 18.4l2.1-2.1M16.3 7.7l2.1-2.1"></path></svg>
          </button>
          <button className="tool-btn" data-tooltip="Narzędzie Zaznaczania" style={{ aspectRatio: 1, background: '#2a2f3a', border: '1px solid #444', cursor: 'pointer', display: 'flex', alignItems: 'center', justifyContent: 'center', borderRadius: '4px' }}>
            <svg viewBox="0 0 24 24" style={{ stroke: '#d4d4d4', width: '24px', height: '24px', fill: 'none', strokeWidth: 2 }}><path d="M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z"></path></svg>
          </button>
          <button className="tool-btn" data-tooltip="Eksport Telemetrii" style={{ aspectRatio: 1, background: '#2a2f3a', border: '1px solid #444', cursor: 'pointer', display: 'flex', alignItems: 'center', justifyContent: 'center', borderRadius: '4px' }}>
            <svg viewBox="0 0 24 24" style={{ stroke: '#d4d4d4', width: '24px', height: '24px', fill: 'none', strokeWidth: 2 }}><path d="M12 3v12M7 10l5 5 5-5M4 21h16"></path></svg>
          </button>
          <button className="tool-btn" data-tooltip="Ziarno z dystansu" style={{ aspectRatio: 1, background: '#2a2f3a', border: '1px solid #444', cursor: 'pointer', display: 'flex', alignItems: 'center', justifyContent: 'center', borderRadius: '4px' }}>
            <svg viewBox="0 0 24 24" style={{ stroke: '#d4d4d4', width: '24px', height: '24px', fill: 'none', strokeWidth: 2 }}><circle cx="6" cy="18" r="2"></circle><path d="M7.5 16.5L20 4M14 4h6v6"></path></svg>
          </button>
        </div>

        <div style={{ padding: '15px', flex: 1 }}>
          <Dashboard agents={snapshot?.agents || []} />
        </div>
      </div>
      
      <main style={{ flex: 1, display: 'flex', justifyContent: 'center', alignItems: 'center', padding: '20px' }}>
        <canvas ref={canvasRef} width={800} height={533} style={{ background: '#1a1c25', border: '1px solid #333', boxShadow: '0 0 20px rgba(0,255,204,0.1)' }} />
      </main>
    </div>
  );
};

export default App;