import React, { useEffect, useRef, useState } from 'react';
import { WebGLRenderer } from './renderer/WebGLRenderer';
import { Dashboard } from './components/Dashboard';
import { useSimulationStore } from './store/useSimulationStore';

const App: React.FC = () => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const canvasWrapRef = useRef<HTMLDivElement>(null);
  const rendererRef = useRef<WebGLRenderer | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const [activeTool, setActiveTool] = useState('seed');
  const snapshot = useSimulationStore((state) => state.snapshot);
  const setSnapshot = useSimulationStore((state) => state.setSnapshot);
  const [selectedUids, setSelectedUids] = useState<string[]>([]);
  const [marquee, setMarquee] = useState<{ sx: number; sy: number; cx: number; cy: number } | null>(null);
  const marqueeStartRef = useRef<{ clientX: number; clientY: number } | null>(null);

  useEffect(() => {
    if (canvasRef.current) { rendererRef.current = new WebGLRenderer(canvasRef.current); }
    const ws = new WebSocket('ws://127.0.0.1:8080');
    wsRef.current = ws;
    ws.onmessage = (event) => { try { setSnapshot(JSON.parse(event.data)); } catch (e) {} };
    return () => ws.close();
  }, [setSnapshot]);

  useEffect(() => {
    if (rendererRef.current && snapshot) { rendererRef.current.render(snapshot, selectedUids); }
  }, [snapshot, selectedUids]);

  // Esc clears the selection and any in-progress marquee.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setSelectedUids([]);
        setMarquee(null);
        marqueeStartRef.current = null;
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const toSim = (clientX: number, clientY: number) => {
    const rect = canvasRef.current!.getBoundingClientRect();
    const x = ((clientX - rect.left) / rect.width) * 32;
    const y = 21 - ((clientY - rect.top) / rect.height) * 21;
    return { x, y };
  };

  const selectInRect = (x0: number, y0: number, x1: number, y1: number) => {
    if (!snapshot) return;
    const minX = Math.min(x0, x1), maxX = Math.max(x0, x1);
    const minY = Math.min(y0, y1), maxY = Math.max(y0, y1);
    const hits: string[] = [];
    snapshot.agents.forEach(a => {
      if (a.pos[0] >= minX && a.pos[0] <= maxX && a.pos[1] >= minY && a.pos[1] <= maxY) {
        hits.push(a.uid);
      }
    });
    (snapshot.predators || []).forEach(p => {
      if (p.pos[0] >= minX && p.pos[0] <= maxX && p.pos[1] >= minY && p.pos[1] <= maxY) {
        hits.push(p.uid);
      }
    });
    setSelectedUids(hits);
  };

  const handleMouseDown = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (activeTool !== 'select') return;
    e.preventDefault();
    marqueeStartRef.current = { clientX: e.clientX, clientY: e.clientY };
    setMarquee({ sx: e.clientX, sy: e.clientY, cx: e.clientX, cy: e.clientY });
  };

  const handleMouseMove = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (!marqueeStartRef.current) return;
    setMarquee(m => (m ? { ...m, cx: e.clientX, cy: e.clientY } : m));
  };

  const handleMouseUp = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (!marqueeStartRef.current) return;
    const start = marqueeStartRef.current;
    marqueeStartRef.current = null;
    setMarquee(null);

    const dx = e.clientX - start.clientX;
    const dy = e.clientY - start.clientY;
    // A sub-3px drag is a plain click: single-point hit-test (agents + predators).
    if (Math.abs(dx) < 3 && Math.abs(dy) < 3) {
      const { x, y } = toSim(e.clientX, e.clientY);
      // Holder object: TS does not narrow `closest` across the closure below.
      const pick: { closest: { uid: string; dist: number } | null } = { closest: null };
      const hit = (uid: string, px: number, py: number) => {
        const dist = Math.sqrt((px - x) ** 2 + (py - y) ** 2);
        if (dist < 1.0 && (!pick.closest || dist < pick.closest.dist)) {
          pick.closest = { uid, dist };
        }
      };
      snapshot?.agents.forEach(a => hit(a.uid, a.pos[0], a.pos[1]));
      (snapshot?.predators || []).forEach(p => hit(p.uid, p.pos[0], p.pos[1]));
      setSelectedUids(pick.closest ? [pick.closest.uid] : []);
      return;
    }

    const s = toSim(start.clientX, start.clientY);
    const c = toSim(e.clientX, e.clientY);
    selectInRect(s.x, s.y, c.x, c.y);
  };

  const handleCanvasClick = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (!canvasRef.current || !wsRef.current) return;
    const { x, y } = toSim(e.clientX, e.clientY);

    if (activeTool === 'seed') {
      wsRef.current.send(`spawn_grain,${x},${y}`);
    } else if (activeTool === 'predator') {
      // 2.2: spawn predator at clicked location (JSON event injection API).
      wsRef.current.send(JSON.stringify({ event: 'spawn_predator', pos: [x, y] }));
    }
  };

  const formatTime = (us: number) => {
    const s = Math.floor(us / 1000000);
    const m = Math.floor(s / 60);
    return `${m.toString().padStart(2, '0')}:${(s % 60).toString().padStart(2, '0')}`;
  };

  const wrapperRect = canvasWrapRef.current?.getBoundingClientRect();
  const marqueeStyle = marquee && activeTool === 'select' && wrapperRect ? {
    left: Math.min(marquee.sx, marquee.cx) - wrapperRect.left,
    top: Math.min(marquee.sy, marquee.cy) - wrapperRect.top,
    width: Math.abs(marquee.cx - marquee.sx),
    height: Math.abs(marquee.cy - marquee.sy),
  } : null;

  return (
    <div style={{ display: 'flex', width: '100vw', height: '100vh', backgroundColor: '#0a0b10', color: '#d4d4d4', fontFamily: 'Courier New' }}>
      <div className="sidebar" style={{ width: '400px', height: '100vh', overflowY: 'auto', background: '#14161f', borderRight: '2px solid #ff6c0c', boxSizing: 'border-box', display: 'flex', flexDirection: 'column' }}>
        <div style={{ padding: '15px', borderBottom: '2px solid #ff6c0c', textAlign: 'center' }}>
          <h2 style={{ margin: 0, color: '#ff6c0c' }}>AVIAN SIM v7.0 PRO</h2>
          <p style={{ margin: '5px 0 0 0', fontSize: '12px', color: '#888' }}>Czas: {snapshot ? formatTime(snapshot.time_us) : "00:00"}</p>
        </div>
        
        {/* Ticket R2-3: Przywrócony toolbar z przełączaniem */}
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: '15px', padding: '15px', borderBottom: '1px solid #333' }}>
          <button onClick={() => setActiveTool('seed')} style={{ aspectRatio: 1, background: activeTool === 'seed' ? '#ff6c0c' : '#2a2f3a', border: `1px solid ${activeTool === 'seed' ? '#ff6c0c' : '#444'}`, cursor: 'pointer', display: 'flex', alignItems: 'center', justifyContent: 'center', borderRadius: '4px' }}>
            <svg viewBox="0 0 24 24" style={{ stroke: activeTool === 'seed' ? '#000' : '#d4d4d4', width: '24px', height: '24px', fill: 'none', strokeWidth: 2 }}><circle cx="12" cy="12" r="3"></circle><path d="M12 3v3M12 18v3M3 12h3M18 12h3M5.6 5.6l2.1 2.1M16.3 16.3l2.1 2.1M5.6 18.4l2.1-2.1M16.3 7.7l2.1-2.1"></path></svg>
          </button>
          <button onClick={() => setActiveTool('select')} style={{ aspectRatio: 1, background: activeTool === 'select' ? '#ff6c0c' : '#2a2f3a', border: `1px solid ${activeTool === 'select' ? '#ff6c0c' : '#444'}`, cursor: 'pointer', display: 'flex', alignItems: 'center', justifyContent: 'center', borderRadius: '4px' }}>
            <svg viewBox="0 0 24 24" style={{ stroke: activeTool === 'select' ? '#000' : '#d4d4d4', width: '24px', height: '24px', fill: 'none', strokeWidth: 2 }}><path d="M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z"></path></svg>
          </button>
          {/* 2.2: spawn a hawk on click */}
          <button onClick={() => setActiveTool('predator')} style={{ aspectRatio: 1, background: activeTool === 'predator' ? '#ff6c0c' : '#2a2f3a', border: `1px solid ${activeTool === 'predator' ? '#ff6c0c' : '#444'}`, cursor: 'pointer', display: 'flex', alignItems: 'center', justifyContent: 'center', borderRadius: '4px' }}>
            <svg viewBox="0 0 24 24" style={{ stroke: activeTool === 'predator' ? '#000' : '#d4d4d4', width: '24px', height: '24px', fill: 'none', strokeWidth: 2 }}><path d="M12 3l6 9-6 9-6-9z"></path></svg>
          </button>
        </div>

        <div style={{ padding: '15px', flex: 1 }}>
          <Dashboard agents={snapshot?.agents || []} selectedUids={selectedUids} lightLevel={snapshot?.light_level} deadCount={snapshot?.dead_count} predatorCount={snapshot?.predators?.length} predators={snapshot?.predators || []} />
        </div>
      </div>
      
      <main style={{ flex: 1, display: 'flex', justifyContent: 'center', alignItems: 'center', padding: '20px' }}>
        <div ref={canvasWrapRef} style={{ position: 'relative' }}>
          <canvas 
            ref={canvasRef} width={800} height={533} onClick={handleCanvasClick}
            onMouseDown={handleMouseDown}
            onMouseMove={handleMouseMove}
            onMouseUp={handleMouseUp}
            style={{ background: '#1a1c25', border: '1px solid #333', boxShadow: '0 0 20px rgba(0,255,204,0.1)', cursor: activeTool === 'select' ? 'crosshair' : 'crosshair' }} 
          />
          {marqueeStyle && (
            <div style={{
              position: 'absolute',
              left: marqueeStyle.left,
              top: marqueeStyle.top,
              width: marqueeStyle.width,
              height: marqueeStyle.height,
              border: '1px dashed #00ffcc',
              background: 'rgba(0,255,204,0.15)',
              pointerEvents: 'none',
              zIndex: 10,
            }} />
          )}
        </div>
      </main>
    </div>
  );
};

export default App;