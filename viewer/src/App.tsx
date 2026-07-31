import React, { useEffect, useRef } from 'react';
import { WebGLRenderer } from './renderer/WebGLRenderer';
import { Dashboard } from './components/Dashboard';
import { useSimulationStore } from './store/useSimulationStore';

const App: React.FC = () => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const rendererRef = useRef<WebGLRenderer | null>(null);
  const snapshot = useSimulationStore((state) => state.snapshot);

  useEffect(() => {
    if (canvasRef.current) {
      rendererRef.current = new WebGLRenderer(canvasRef.current);
    }
  }, []);

  useEffect(() => {
    if (rendererRef.current && snapshot) {
      rendererRef.current.render(snapshot);
    }
  }, [snapshot]);

  return (
    <div style={{ display: 'flex' }}>
      <canvas ref={canvasRef} width={800} height={533} />
      <Dashboard agents={snapshot?.agents || []} />
    </div>
  );
};

export default App;
