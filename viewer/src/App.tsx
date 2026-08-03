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
  const appendEvents = useSimulationStore((state) => state.appendEvents);
  const setMetrics = useSimulationStore((state) => state.setMetrics);
  const metrics = useSimulationStore((state) => state.metrics);
  const eventLog = useSimulationStore((state) => state.eventLog);
  const [selectedUids, setSelectedUids] = useState<string[]>([]);
  const [marquee, setMarquee] = useState<{ sx: number; sy: number; cx: number; cy: number } | null>(null);
  const marqueeStartRef = useRef<{ clientX: number; clientY: number } | null>(null);
  // 6.4: drag-to-pan anchor (middle-button, or left-button while zoomed).
  const panRef = useRef<{ x: number; y: number } | null>(null);
  // 6.1: per-agent hover info + time controls (pause/step/speed presets).
  const [hoveredUid, setHoveredUid] = useState<string | null>(null);
  const [hoverPos, setHoverPos] = useState<{ x: number; y: number } | null>(null);
  const [paused, setPaused] = useState(false);
  const [speed, setSpeed] = useState(1);
  // 6.4: live connection status (reconnect-with-backoff) + client-side FPS.
  const [connected, setConnected] = useState(false);
  // Audit 4 §9.2: two distinct numbers — `paintFps` is the real rAF frame rate
  // (what the user perceives), `recvFps` counts WS snapshot arrivals. The
  // primary "fps" label now reflects paint performance, since a slow client
  // can still receive 60 snapshots/s while visibly stuttering.
  const [paintFps, setPaintFps] = useState(0);
  const [recvFps, setRecvFps] = useState(0);
  const recvFpsRef = useRef({ frames: 0, last: performance.now() });
  const reconnectDelayRef = useRef(1000);
  // Audit 2 Task 2: flock/neighbor connection line visibility (default on).
  const [neighborLinesVisible, setNeighborLinesVisible] = useState(true);
  // Sprint 5 (background task item 4): mobile — the sidebar is an overlay
  // drawer on narrow screens; this toggles it open/closed.
  const [sidebarOpen, setSidebarOpen] = useState(false);

  useEffect(() => {
    if (canvasRef.current) { rendererRef.current = new WebGLRenderer(canvasRef.current); }
    let ws: WebSocket | null = null;
    let closed = false;

    const connect = () => {
      if (closed) return;
      ws = new WebSocket('ws://127.0.0.1:8080');
      wsRef.current = ws;
      ws.onopen = () => {
        reconnectDelayRef.current = 1000;
        setConnected(true);
      };
      ws.onclose = () => {
        setConnected(false);
        if (!closed) {
          // 6.4: reconnect with exponential backoff (1s → 2s → 4s → … cap 15s).
          setTimeout(connect, reconnectDelayRef.current);
          reconnectDelayRef.current = Math.min(reconnectDelayRef.current * 2, 15000);
        }
      };
      ws.onmessage = (event) => {
        try {
          const data = JSON.parse(event.data);
          // Audit 4 §9.6: the server sends ONE coalesced message per broadcast
          // tick — `{ snapshot?, event_log?, metrics? }` — instead of up to 3
          // separate `type`-keyed messages. Handle each field independently.
          if (data && data.snapshot) {
            setSnapshot(data.snapshot);
            // Audit 4 §9.2: recv fps = snapshot frames received per second.
            const now = performance.now();
            const f = recvFpsRef.current;
            f.frames += 1;
            if (now - f.last >= 500) {
              setRecvFps(Math.round((f.frames * 1000) / (now - f.last)));
              f.frames = 0;
              f.last = now;
            }
          }
          if (data && data.event_log) {
            // 6.1: scenario event log (frame + injected events).
            appendEvents(data.event_log.frame, data.event_log.events);
          }
          if (data && data.metrics) {
            // 6.2: dashboard metrics (every ~100 frames).
            setMetrics(data.metrics);
          }
        } catch (e) {}
      };
    };
    connect();
    return () => {
      closed = true;
      ws?.close();
    };
  }, [setSnapshot, appendEvents, setMetrics]);

  // Audit 3 Phase 4: continuous requestAnimationFrame render loop. It reads the
  // store's double buffer (previous/current snapshot + arrival timestamps) via
  // getState(), so React is never re-rendered by the loop — the renderer
  // lerps/slerps positions and headings between the two snapshots, keeping a
  // smooth ~60fps even when the server drops frames or WS packets burst.
  const selectedUidsRef = useRef<string[]>(selectedUids);
  useEffect(() => { selectedUidsRef.current = selectedUids; }, [selectedUids]);
  const hoveredUidRef = useRef<string | null>(hoveredUid);
  useEffect(() => { hoveredUidRef.current = hoveredUid; }, [hoveredUid]);

  useEffect(() => {
    let raf = 0;
    // Audit 4 §9.3: requestAnimationFrame is vsync-limited (~60 fps max) —
    // this is the browser-side 60fps cap. Do NOT replace with setInterval.
    const loop = () => {
      const r = rendererRef.current;
      if (r) {
        const st = useSimulationStore.getState();
        if (st.snapshot) {
          r.render(
            st.snapshot,
            st.previousSnapshot,
            st.lastReceivedAt,
            st.currentReceivedAt,
            selectedUidsRef.current,
            hoveredUidRef.current || undefined,
          );
          // Audit 4 §9.2: primary fps label = actual paint rate (rAF clock).
          setPaintFps(r.paintFps());
        }
      }
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(raf);
  }, []);

  const sendControl = (command: string, value?: number) => {
    const msg: Record<string, unknown> = { command };
    if (value !== undefined) msg.value = value;
    wsRef.current?.send(JSON.stringify(msg));
  };

  // Esc clears the selection and any in-progress marquee.
  // Space = pause/play, S = single-step (6.1 time controls).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setSelectedUids([]);
        setMarquee(null);
        marqueeStartRef.current = null;
      } else if (e.key === ' ') {
        e.preventDefault();
        setPaused((p) => { sendControl(p ? 'resume' : 'pause'); return !p; });
      } else if (e.key === 's' || e.key === 'S') {
        setPaused((p) => {
          if (p) { sendControl('step'); }
          return p;
        });
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  const toSim = (clientX: number, clientY: number) => {
    const rect = canvasRef.current!.getBoundingClientRect();
    // 6.4: respect the current zoom/pan camera.
    const r = rendererRef.current!;
    return r.screenToWorld(clientX - rect.left, clientY - rect.top, rect.width, rect.height);
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
    // 6.4: middle button always pans; left-button pans when zoomed in, else
    // starts a marquee selection.
    const zoomed = rendererRef.current ? rendererRef.current.viewZoomRef() > 1 : false;
    const panMode = e.button === 1 || (e.button === 0 && zoomed);
    if (panMode) {
      panRef.current = { x: e.clientX, y: e.clientY };
      return;
    }
    marqueeStartRef.current = { clientX: e.clientX, clientY: e.clientY };
    setMarquee({ sx: e.clientX, sy: e.clientY, cx: e.clientX, cy: e.clientY });
  };

  const handleMouseMove = (e: React.MouseEvent<HTMLCanvasElement>) => {
    // 6.4: drag-to-pan while the pan anchor is set.
    if (panRef.current && rendererRef.current) {
      const rect = canvasRef.current!.getBoundingClientRect();
      const dx = (e.clientX - panRef.current.x) / rect.width * (32 / rendererRef.current.viewZoomRef());
      const dy = (e.clientY - panRef.current.y) / rect.height * (21 / rendererRef.current.viewZoomRef());
      rendererRef.current.panBy(-dx, -dy);
      panRef.current = { x: e.clientX, y: e.clientY };
      return;
    }
    if (marqueeStartRef.current) {
      setMarquee(m => (m ? { ...m, cx: e.clientX, cy: e.clientY } : m));
    }
    // 6.1: per-agent info on hover — nearest agent/predator within ~0.8 m.
    const { x, y } = toSim(e.clientX, e.clientY);
    // Holder object: TS does not narrow a plain `let` mutated inside a closure.
    const pick: { closest: { uid: string; dist: number } | null } = { closest: null };
    const consider = (uid: string, px: number, py: number) => {
      const dist = Math.sqrt((px - x) ** 2 + (py - y) ** 2);
      if (dist < 0.8 && (!pick.closest || dist < pick.closest.dist)) pick.closest = { uid, dist };
    };
    snapshot?.agents.forEach(a => consider(a.uid, a.pos[0], a.pos[1]));
    (snapshot?.predators || []).forEach(p => consider(p.uid, p.pos[0], p.pos[1]));
    setHoveredUid(pick.closest ? pick.closest.uid : null);
    setHoverPos(pick.closest ? { x: e.clientX, y: e.clientY } : null);
  };

  const handleMouseLeave = () => {
    setHoveredUid(null);
    setHoverPos(null);
  };

  // 6.4: mouse-wheel zoom centered on the cursor position.
  const handleWheel = (e: React.WheelEvent<HTMLCanvasElement>) => {
    e.preventDefault();
    const rect = canvasRef.current!.getBoundingClientRect();
    const px = e.clientX - rect.left;
    const py = e.clientY - rect.top;
    const world = toSim(e.clientX, e.clientY);
    const factor = e.deltaY < 0 ? 1.2 : 1 / 1.2;
    const renderer = rendererRef.current!;
    renderer.setZoom(renderer.viewZoomRef() * factor, world.x, world.y);
  };

  const handleMouseUp = (e: React.MouseEvent<HTMLCanvasElement>) => {
    if (panRef.current) {
      panRef.current = null;
      return;
    }
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

  // 6.1: hovered entity for the tooltip (agent or predator).
  const hoveredAgent = hoveredUid ? snapshot?.agents.find(a => a.uid === hoveredUid) : null;
  const hoveredPredator = hoveredUid ? snapshot?.predators.find(p => p.uid === hoveredUid) : null;
  const tooltipRect = hoverPos && wrapperRect ? {
    left: hoverPos.x - wrapperRect.left + 14,
    top: hoverPos.y - wrapperRect.top + 14,
  } : null;

  const controlBtn = (base: React.CSSProperties): React.CSSProperties => ({
    flex: 1,
    padding: '6px 0',
    fontSize: '12px',
    cursor: 'pointer',
    borderRadius: '4px',
    border: '1px solid #444',
    background: '#2a2f3a',
    color: '#d4d4d4',
    ...base,
  });

  return (
    <div className="app-root" style={{ display: 'flex', width: '100vw', height: '100vh', backgroundColor: 'transparent', color: '#d4d4d4', fontFamily: 'Courier New' }}>
      {/* Sprint 5 (background task item 4): mobile hamburger to open the
          sidebar drawer (hidden on desktop via CSS). */}
      <button
        className="sidebar-toggle"
        onClick={() => setSidebarOpen(o => !o)}
        title="Toggle panel"
        aria-label="Toggle panel"
      >
        {sidebarOpen ? '✕' : '☰'}
      </button>
      <div className={`sidebar${sidebarOpen ? ' open' : ''}`} style={{ width: '340px', height: '100vh', overflowY: 'auto', background: '#14161f', borderRight: '2px solid #ff6c0c', boxSizing: 'border-box', display: 'flex', flexDirection: 'column' }}>
        <div style={{ padding: '12px 15px', borderBottom: '2px solid #ff6c0c', textAlign: 'center' }}>
          <h2 style={{ margin: 0, color: '#ff6c0c', fontSize: '16px' }}>AVIAN SIM v7.0 PRO</h2>
          {/* 6.4: live status bar — connection + fps + sim time + agent/grain counts. */}
          <div style={{ margin: '6px 0 0 0', fontSize: '11px', display: 'flex', justifyContent: 'center', gap: '10px', color: '#888' }}>
            <span style={{ color: connected ? '#00ffcc' : '#ff3366' }}>{connected ? '● CONNECTED' : '○ DISCONNECTED'}</span>
            <span>{paintFps} fps paint</span>
            <span>{recvFps} recv/s</span>
            <span>#{snapshot?.frame ?? 0}</span>
            <span>{snapshot ? formatTime(snapshot.time_us) : "00:00"}</span>
            <span>{snapshot?.agents.length ?? 0} birds</span>
            <span>{snapshot?.grains.length ?? 0} grains</span>
          </div>
        </div>
        
        {/* Ticket R2-3: Toolbar with tool switching (seed / select / predator). */}
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: '10px', padding: '12px', borderBottom: '1px solid #333' }}>
          <button onClick={() => setActiveTool('seed')} title="Seed: click to drop grain" style={{ aspectRatio: 1, background: activeTool === 'seed' ? '#ff6c0c' : '#2a2f3a', border: `1px solid ${activeTool === 'seed' ? '#ff6c0c' : '#444'}`, cursor: 'pointer', display: 'flex', alignItems: 'center', justifyContent: 'center', borderRadius: '4px' }}>
            <svg viewBox="0 0 24 24" style={{ stroke: activeTool === 'seed' ? '#000' : '#d4d4d4', width: '22px', height: '22px', fill: 'none', strokeWidth: 2 }}><circle cx="12" cy="12" r="3"></circle><path d="M12 3v3M12 18v3M3 12h3M18 12h3M5.6 5.6l2.1 2.1M16.3 16.3l2.1 2.1M5.6 18.4l2.1-2.1M16.3 7.7l2.1-2.1"></path></svg>
          </button>
          <button onClick={() => setActiveTool('select')} title="Select: drag a box or click a bird" style={{ aspectRatio: 1, background: activeTool === 'select' ? '#ff6c0c' : '#2a2f3a', border: `1px solid ${activeTool === 'select' ? '#ff6c0c' : '#444'}`, cursor: 'pointer', display: 'flex', alignItems: 'center', justifyContent: 'center', borderRadius: '4px' }}>
            <svg viewBox="0 0 24 24" style={{ stroke: activeTool === 'select' ? '#000' : '#d4d4d4', width: '22px', height: '22px', fill: 'none', strokeWidth: 2 }}><path d="M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z"></path></svg>
          </button>
          {/* 2.2: spawn a hawk on click */}
          <button onClick={() => setActiveTool('predator')} title="Predator: click to spawn a hawk" style={{ aspectRatio: 1, background: activeTool === 'predator' ? '#ff6c0c' : '#2a2f3a', border: `1px solid ${activeTool === 'predator' ? '#ff6c0c' : '#444'}`, cursor: 'pointer', display: 'flex', alignItems: 'center', justifyContent: 'center', borderRadius: '4px' }}>
            <svg viewBox="0 0 24 24" style={{ stroke: activeTool === 'predator' ? '#000' : '#d4d4d4', width: '22px', height: '22px', fill: 'none', strokeWidth: 2 }}><path d="M12 3l6 9-6 9-6-9z"></path></svg>
          </button>
        </div>

        {/* 6.1: transport time controls (pause / step / speed presets). */}
        <div style={{ padding: '8px 12px', borderBottom: '1px solid #333', display: 'flex', flexDirection: 'column', gap: '6px' }}>
          <div style={{ display: 'flex', gap: '6px' }}>
            <button onClick={() => { const p = !paused; setPaused(p); sendControl(p ? 'pause' : 'resume'); }} style={controlBtn({ background: paused ? '#ff6c0c' : '#2a2f3a', color: paused ? '#000' : '#d4d4d4' })}>
              {paused ? '▶ PLAY' : '⏸ PAUSE'}
            </button>
            <button onClick={() => { if (paused) { sendControl('step'); } }} style={controlBtn({ opacity: paused ? 1 : 0.4 })} title="Step one frame">
              STEP
            </button>
          </div>
          <div style={{ display: 'flex', gap: '6px' }}>
            {[1, 10, 100].map((s) => (
              <button key={s} onClick={() => { setSpeed(s); sendControl('speed', s); }} style={controlBtn({ background: speed === s ? '#ff6c0c' : '#2a2f3a', color: speed === s ? '#000' : '#d4d4d4' })}>
                {s}×
              </button>
            ))}
          </div>
          {/* Audit 2 Task 2: flock/neighbor connection lines toggle (default on). */}
          <label style={{ display: 'flex', alignItems: 'center', gap: '6px', fontSize: '11px', color: '#d4d4d4', cursor: 'pointer' }}>
            <input
              type="checkbox"
              checked={neighborLinesVisible}
              onChange={(e) => {
                const v = e.target.checked;
                setNeighborLinesVisible(v);
                rendererRef.current?.setNeighborLinesVisible(v);
              }}
            />
            Flock lines
          </label>
        </div>

        <div style={{ padding: '12px', flex: 1 }}>
          <Dashboard agents={snapshot?.agents || []} selectedUids={selectedUids} lightLevel={snapshot?.light_level} deadCount={snapshot?.dead_count} predatorCount={snapshot?.predators?.length} predators={snapshot?.predators || []} metrics={metrics} onSelectUid={(uid) => setSelectedUids([uid])} />
        </div>

        {/* 6.1: event log panel — last injected scenario events. */}
        <div style={{ borderTop: '1px solid #333', padding: '10px 12px', maxHeight: '150px', overflowY: 'auto', background: '#10121a' }}>
          <h3 style={{ margin: '0 0 6px 0', fontSize: '12px', color: '#00ffcc' }}>EVENT LOG</h3>
          {eventLog.length === 0 && <p style={{ color: '#666', fontSize: '11px', margin: 0 }}>No events yet</p>}
          {eventLog.slice(-30).reverse().map((entry, i) => (
            <div key={`${entry.frame}-${i}`} style={{ fontSize: '10px', color: '#888', display: 'flex', gap: '8px' }}>
              <span style={{ color: '#ff6c0c' }}>#{entry.frame}</span>
              <span style={{ color: '#00ffcc' }}>{entry.event}</span>
            </div>
          ))}
        </div>
      </div>
      
      <main style={{ flex: 1, display: 'flex', justifyContent: 'center', alignItems: 'center', padding: '20px' }}>
        <div ref={canvasWrapRef} style={{ position: 'relative' }}>
          <canvas 
            ref={canvasRef} width={800} height={533} onClick={handleCanvasClick}
            onMouseDown={handleMouseDown}
            onMouseMove={handleMouseMove}
            onMouseUp={handleMouseUp}
            onMouseLeave={handleMouseLeave}
            onWheel={handleWheel}
            style={{ background: '#1a1c25', border: '1px solid #333', boxShadow: '0 0 20px rgba(0,255,204,0.1)', cursor: 'crosshair' }} 
          />
          {/* 6.4: viewport reset — back to the full 32×21 arena view. */}
          <button onClick={() => { rendererRef.current?.resetView(); }} title="Reset view (full arena)" style={{
            position: 'absolute', top: '8px', right: '8px', zIndex: 12,
            background: '#2a2f3a', color: '#d4d4d4', border: '1px solid #444',
            borderRadius: '4px', padding: '3px 8px', fontSize: '11px', cursor: 'pointer',
          }}>RESET VIEW</button>
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
          {tooltipRect && (hoveredAgent || hoveredPredator) && (
            <div style={{
              position: 'absolute',
              left: tooltipRect.left,
              top: tooltipRect.top,
              background: 'rgba(10,11,16,0.95)',
              border: `1px solid ${hoveredAgent ? '#00ffcc' : '#ff2222'}`,
              padding: '6px 9px',
              borderRadius: '4px',
              fontSize: '11px',
              lineHeight: '1.5',
              pointerEvents: 'none',
              zIndex: 20,
              whiteSpace: 'nowrap',
            }}>
              {hoveredAgent ? (
                <>
                  <div style={{ color: '#00ffcc', fontWeight: 'bold' }}>PIGEON: {hoveredAgent.uid}</div>
                  <div>FSM: <span style={{ color: '#ffcc00' }}>{hoveredAgent.fsm_state}</span></div>
                  <div>E: {hoveredAgent.energy_kj.toFixed(1)} kJ · H: {(hoveredAgent.hunger * 100).toFixed(0)}%</div>
                  <div>Age: {hoveredAgent.age_years.toFixed(1)}y · Mass: {hoveredAgent.mass_g.toFixed(0)} g</div>
                  <div>Vitality: {hoveredAgent.vitality?.toFixed(2) ?? '—'}{hoveredAgent.sick ? ' · 🦠 SICK' : ''}</div>
                  {hoveredAgent.alarm_triggered && <div style={{ color: '#ff2222' }}>🚨 ALARM</div>}
                </>
              ) : (
                <>
                  <div style={{ color: '#ff2222', fontWeight: 'bold' }}>PREDATOR: {hoveredPredator!.uid}</div>
                  <div>Expires in: {hoveredPredator!.lifetime_remaining_s.toFixed(1)} s</div>
                  <div>Hunt: <span style={{ color: '#ffcc00' }}>{hoveredPredator!.hunt_state}</span> · Speed: {hoveredPredator!.speed_level}/5</div>
                  <div>Meals: {hoveredPredator!.meals_eaten}/3</div>
                </>
              )}
            </div>
          )}
          {!snapshot && (
            <div style={{
              position: 'absolute',
              inset: 0,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              background: 'rgba(10,11,16,0.85)',
              zIndex: 15,
              pointerEvents: 'none',
              textAlign: 'center',
              padding: '20px',
            }}>
              <div>
                <div style={{ color: '#ff3366', fontSize: '16px', marginBottom: '8px' }}>Simulation not running</div>
                <div style={{ color: '#888', fontSize: '12px' }}>Start the Rust server (cargo run --bin sim_server) to begin.</div>
              </div>
            </div>
          )}
        </div>
      </main>
    </div>
  );
};

export default App;