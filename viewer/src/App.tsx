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
    ws.onerror = () => console.error("❌ Błąd WebSocket. Czy serwer Rust działa?");

    return () => ws.close();
  }, [setSnapshot]);

  useEffect(() => {
    if (rendererRef.current && snapshot) {
      rendererRef.current.render(snapshot);
    }
  }, [snapshot]);

  return (
    <div style={{ display: 'flex', padding: '20px', gap: '20px' }}>
      <canvas ref={canvasRef} width={800} height={533} />
      <div className="sidebar">
        <div className="sidebar-header">
          <h2>AVIAN SIM v7.0 PRO</h2>
          <p>Ground Truth Telemetry | Rust Core + React Viewer</p>
        </div>
        <Dashboard agents={snapshot?.agents || []} />
      </div>
    </div>
  );
};

export default App;