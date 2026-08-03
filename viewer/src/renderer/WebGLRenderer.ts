import * as THREE from 'three';

// 6.5: FSM → accent color (same mapping as the Dashboard compact cards).
const FSM_COLORS: Record<string, number> = {
  Idle: 0xaaaaaa,
  Foraging: 0x00ff00,
  Fleeing: 0xff2222,
  Scanning: 0x00e5ff,
  Spacer: 0x888888,
  Preening: 0x00e5ff,
  Sick: 0xcc44ff,
  Roosting: 0x6b5bff,
};

// 6.5: morph variants — gray / brown feral / white, chosen per-agent from the UID.
const MORPH_COLORS = [0x9a9aa0, 0x8a6a4a, 0xe0e0e0];

export class WebGLRenderer {
  private scene: THREE.Scene;
  private camera: THREE.OrthographicCamera;
  private renderer: THREE.WebGLRenderer;
  // Sprint 5 (background task): repo-owned photo (`viewer/public/assets/
  // background-no-fireflies.jpg`) rendered as the arena floor backdrop behind
  // the pigeons, not as a CSS window background.
  private backgroundMesh: THREE.Mesh | null = null;
  private backgroundTexture: THREE.Texture | null = null;
  private birdBodyMesh: THREE.InstancedMesh;
  private birdHeadMesh: THREE.InstancedMesh;
  private birdWingLMesh: THREE.InstancedMesh;
  private birdWingRMesh: THREE.InstancedMesh;
  private birdShadowMesh: THREE.InstancedMesh;
  private stateRingMesh: THREE.InstancedMesh;
  private grainMesh: THREE.InstancedMesh;
  private predatorMesh: THREE.InstancedMesh;
  private nightOverlay: THREE.Mesh;
  private weatherOverlay: THREE.Mesh;
  private obstacleGroup: THREE.Group;
  // Audit 2 Task 4: predator lifetime labels cached per uid — the canvas +
  // texture are only rebuilt when the displayed value changes, not every frame.
  private predatorLabels = new Map<string, { sprite: THREE.Sprite; text: string; texture: THREE.CanvasTexture }>();
  private selectionMarkers: THREE.Mesh[] = [];
  // 6.1: FOV cone pool — one mesh per hovered/selected agent, reused (no per-frame GC).
  private fovCones: THREE.Mesh[] = [];
  private fovConeGeom: THREE.ShapeGeometry;
  private fovConeMat: THREE.MeshBasicMaterial;
  // 6.1: flock viz — line segments between agents within the flock radius.
  private neighborLines: THREE.LineSegments;
  private neighborGeom: THREE.BufferGeometry;
  // Audit 2 Task 2: flock/neighbor line toggle. Default on (matches previous
  // behavior); when off, both the rendering AND the per-frame pair scan are
  // skipped.
  private neighborLinesVisible = true;
  // 6.1: memory dots — instanced fading dots at remembered food locations.
  private memoryDots: THREE.InstancedMesh;
  // 6.4: viewport camera — zoom (scale factor) + center pan (world units).
  private viewZoom = 1;
  private viewCenter = { x: 16, y: 10.5 };
  // 6.5: animation clock (seconds) for wing flap + peck pulses.
  private animTime = 0;
  private lastFrameMs = performance.now();
  // Audit 4 §9.2: actual PAINT fps, measured from the same rAF clock that
  // drives animTime (no second clock). Rolling average over a 500 ms window.
  private paintFrames = 0;
  private paintWindowStart = performance.now();
  private paintFpsValue = 0;
  // 6.6: pooled dummy objects — created once, reused every frame (zero
  // `new Object3D` during render).
  private dummyBody = new THREE.Object3D();
  private dummyHead = new THREE.Object3D();
  private dummyWingL = new THREE.Object3D();
  private dummyWingR = new THREE.Object3D();
  private dummyShadow = new THREE.Object3D();
  private dummyRing = new THREE.Object3D();
  private dummyGrain = new THREE.Object3D();
  private dummyPred = new THREE.Object3D();
  private scratchColor = new THREE.Color();
  // 6.6: dirty-flag cache — only recompute an instance matrix when
  // pos/heading/mass/head actually changed since the last snapshot.
  private agentCache = new Map<string, { pos: [number, number]; heading: number; mass: number; bob: [number, number]; fsm: string }>();
  // Audit 4 §9.1: bounded index-based dirty caches for grains/predators (raw
  // Vecs are position-ordered and never reorder). No uid-keyed growth, no
  // per-frame string keys.
  private lastGrains: Array<[number, number]> | null = null;
  private lastPreds: any[] | null = null;

  // Visual constants (renderer-only; ground truth lives in calibration.rs).
  private static readonly FOV_CONE_RADIUS = 3.0;
  private static readonly FOV_CONE_OPACITY = 0.15;
  private static readonly FLOCK_LINE_RADIUS = 3.0;
  private static readonly MEMORY_DOT_CAPACITY = 500;
  private static readonly WORLD_W = 32;
  private static readonly WORLD_H = 21;
  private static readonly INSTANCE_CAPACITY = 2000;
  // 6.5: wing flap — amplitude scales with speed, near-zero when idle (tucked).
  private static readonly FLAP_MIN_SPEED_MS = 0.3;
  private static readonly FLAP_FREQ_BASE = 4.0;
  private static readonly FLAP_FREQ_PER_SPEED = 4.0;
  // 6.5: peck — reuse the HeadBobSystem jerk curve, longer thrust near grain.
  private static readonly PECK_NEAR_GRAIN_M = 0.9;
  private static readonly PECK_EXTRA_M = 0.16;
  private static readonly PECK_FREQ_HZ = 4.0;

  // 6.5: teardrop body (rounded front + pointed tail), pointing +X.
  private static buildTeardropGeometry(): THREE.ShapeGeometry {
    const s = new THREE.Shape();
    s.moveTo(0.35, 0);
    s.quadraticCurveTo(0.45, 0.18, 0.25, 0.28);
    s.quadraticCurveTo(0.0, 0.36, -0.2, 0.22);
    s.quadraticCurveTo(-0.3, 0.1, -0.35, 0.05);
    s.quadraticCurveTo(-0.42, 0.0, -0.35, -0.05);
    s.quadraticCurveTo(-0.3, -0.1, -0.2, -0.22);
    s.quadraticCurveTo(0.0, -0.36, 0.25, -0.28);
    s.quadraticCurveTo(0.45, -0.18, 0.35, 0);
    return new THREE.ShapeGeometry(s);
  }

  // 6.5: small leaf wing anchored at the body, sweeping in z to "flap".
  private static buildWingGeometry(): THREE.ShapeGeometry {
    const s = new THREE.Shape();
    s.moveTo(0, 0);
    s.lineTo(0.3, 0.1);
    s.lineTo(0.32, 0);
    s.lineTo(0.3, -0.1);
    s.closePath();
    return new THREE.ShapeGeometry(s);
  }

  // Audit 3 Phase 4: interpolate one entity between the previous and current
  // WS snapshots at alpha ∈ [0,1]. Position is lerped; heading is slerped
  // (shortest-arc) so a bird spinning past ±π never whips the long way around.
  // Returns a lightweight copy of the current snapshot entry carrying the
  // interpolated pos/heading — every other field is shared, not cloned.
  private static resolveAgent(prev: any, curr: any, alpha: number): any {
    const pos: [number, number] = [
      prev.pos[0] + (curr.pos[0] - prev.pos[0]) * alpha,
      prev.pos[1] + (curr.pos[1] - prev.pos[1]) * alpha,
    ];
    const twoPi = Math.PI * 2;
    let d = curr.heading - prev.heading;
    d = (((d + Math.PI) % twoPi) + twoPi) % twoPi - Math.PI; // wrap to (-π, π]
    const heading = prev.heading + d * alpha;
    return { ...curr, pos, heading };
  }

  constructor(canvas: HTMLCanvasElement) {
    this.scene = new THREE.Scene();
    this.camera = new THREE.OrthographicCamera(0, 32, 21, 0, 0.1, 1000);
    this.camera.position.z = 10;

    this.renderer = new THREE.WebGLRenderer({ canvas, antialias: true });
    this.renderer.setSize(800, 533);
    this.renderer.setClearColor(0x1a1c25);

    // Sprint 5 (background task): the photo fills the arena where the pigeons
    // walk. A single plane behind the grid; the texture is loaded async and
    // covers the full 32×21 world at a "cover" aspect (crop the wider axis).
    const bgLoader = new THREE.TextureLoader();
    bgLoader.setCrossOrigin('anonymous');
    this.backgroundTexture = bgLoader.load('/assets/background-no-fireflies.jpg');
    const bgMat = new THREE.MeshBasicMaterial({
      map: this.backgroundTexture,
      depthTest: false,
      depthWrite: false,
      color: 0xffffff,
    });
    // Cover: fit the shorter world axis, crop the overflow on the other.
    const img = 1664 / 928; // source aspect ratio (width / height)
    const world = WebGLRenderer.WORLD_W / WebGLRenderer.WORLD_H;
    let bw = WebGLRenderer.WORLD_W;
    let bh = WebGLRenderer.WORLD_H;
    if (img > world) {
      bw = WebGLRenderer.WORLD_H * img; // wider → crop left/right
    } else {
      bh = WebGLRenderer.WORLD_W / img; // taller → crop top/bottom
    }
    const bgGeom = new THREE.PlaneGeometry(bw, bh);
    this.backgroundMesh = new THREE.Mesh(bgGeom, bgMat);
    this.backgroundMesh.position.set(WebGLRenderer.WORLD_W / 2, WebGLRenderer.WORLD_H / 2, -4);
    this.backgroundMesh.renderOrder = -1;
    this.scene.add(this.backgroundMesh);

    const grid = new THREE.GridHelper(64, 64, 0x333333, 0x222222);
    grid.rotation.x = Math.PI / 2;
    grid.position.set(16, 10.5, -1);
    this.scene.add(grid);

    // 6.5: per-agent ground shadow (soft dark ellipse, slightly larger than body).
    const shadowGeom = new THREE.CircleGeometry(0.46, 20);
    const shadowMat = new THREE.MeshBasicMaterial({ color: 0x000000, transparent: true, opacity: 0.32, depthTest: false });
    this.birdShadowMesh = new THREE.InstancedMesh(shadowGeom, shadowMat, WebGLRenderer.INSTANCE_CAPACITY);
    this.birdShadowMesh.frustumCulled = false;
    this.birdShadowMesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
    this.scene.add(this.birdShadowMesh);

    // 6.5: teardrop body (rounded + pointed tail) instead of a plain circle.
    const bodyGeom = WebGLRenderer.buildTeardropGeometry();
    const bodyMat = new THREE.MeshBasicMaterial({ color: 0xff6c0c });
    this.birdBodyMesh = new THREE.InstancedMesh(bodyGeom, bodyMat, WebGLRenderer.INSTANCE_CAPACITY);
    this.birdBodyMesh.frustumCulled = false;
    this.birdBodyMesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
    this.scene.add(this.birdBodyMesh);

    // 6.5: head — small shape offset forward, carrying head-bob + peck thrust.
    const headGeom = new THREE.CircleGeometry(0.13, 16);
    const headMat = new THREE.MeshBasicMaterial({ color: 0xffffff });
    this.birdHeadMesh = new THREE.InstancedMesh(headGeom, headMat, WebGLRenderer.INSTANCE_CAPACITY);
    this.birdHeadMesh.frustumCulled = false;
    this.birdHeadMesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
    this.scene.add(this.birdHeadMesh);

    // 6.5: two wings — flap amplitude scales with speed, tucked when idle.
    const wingGeom = WebGLRenderer.buildWingGeometry();
    const wingMat = new THREE.MeshBasicMaterial({ color: 0xd8d8d8 });
    this.birdWingLMesh = new THREE.InstancedMesh(wingGeom, wingMat, WebGLRenderer.INSTANCE_CAPACITY);
    this.birdWingLMesh.frustumCulled = false;
    this.birdWingLMesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
    this.scene.add(this.birdWingLMesh);
    this.birdWingRMesh = new THREE.InstancedMesh(wingGeom, wingMat.clone(), WebGLRenderer.INSTANCE_CAPACITY);
    this.birdWingRMesh.frustumCulled = false;
    this.birdWingRMesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
    this.scene.add(this.birdWingRMesh);

    // 6.5: FSM state ring — thin outline under each bird, colored by state.
    const ringGeom = new THREE.RingGeometry(0.52, 0.62, 24);
    const ringMat = new THREE.MeshBasicMaterial({ transparent: true, opacity: 0.55, depthTest: false, side: THREE.DoubleSide });
    this.stateRingMesh = new THREE.InstancedMesh(ringGeom, ringMat, WebGLRenderer.INSTANCE_CAPACITY);
    this.stateRingMesh.frustumCulled = false;
    this.stateRingMesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
    this.scene.add(this.stateRingMesh);

    const grainGeom = new THREE.CircleGeometry(0.2, 8);
    const grainMat = new THREE.MeshBasicMaterial({ color: 0xffff00 });
    this.grainMesh = new THREE.InstancedMesh(grainGeom, grainMat, WebGLRenderer.INSTANCE_CAPACITY);
    this.grainMesh.frustumCulled = false;
    this.grainMesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
    this.scene.add(this.grainMesh);

    // 2.2: predators rendered as larger red triangles (hawk).
    const predGeom = new THREE.CircleGeometry(0.55, 16);
    const predMat = new THREE.MeshBasicMaterial({ color: 0xff2222 });
    this.predatorMesh = new THREE.InstancedMesh(predGeom, predMat, 64);
    this.predatorMesh.frustumCulled = false;
    this.predatorMesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
    this.scene.add(this.predatorMesh);

    // 2.3: black full-screen overlay; opacity = 1 - light_level → dims at night.
    const overlayMat = new THREE.MeshBasicMaterial({
      color: 0x000000,
      transparent: true,
      opacity: 0,
      depthTest: false,
    });
    this.nightOverlay = new THREE.Mesh(new THREE.PlaneGeometry(32, 21), overlayMat);
    this.nightOverlay.position.set(16, 10.5, 5);
    this.nightOverlay.renderOrder = 999;
    this.scene.add(this.nightOverlay);

    // 4.4: weather tint — rain dims blue, heat warms amber, wind stays clear.
    const weatherMat = new THREE.MeshBasicMaterial({
      color: 0xffffff,
      transparent: true,
      opacity: 0,
      depthTest: false,
    });
    this.weatherOverlay = new THREE.Mesh(new THREE.PlaneGeometry(32, 21), weatherMat);
    this.weatherOverlay.position.set(16, 10.5, 4);
    this.weatherOverlay.renderOrder = 998;
    this.scene.add(this.weatherOverlay);

    // 4.3: static urban obstacles (buildings/trees/water), rebuilt per frame.
    this.obstacleGroup = new THREE.Group();
    this.scene.add(this.obstacleGroup);

    // 6.1: FOV cone geometry — a wedge spanning ±VISION_FOV_DEGREES/2 about the
    // +X axis (agent heading). Built once, shared by the cone pool meshes.
    const fovHalf = (340.0 / 2) * (Math.PI / 180);
    const coneShape = new THREE.Shape();
    coneShape.moveTo(0, 0);
    coneShape.absarc(0, 0, WebGLRenderer.FOV_CONE_RADIUS, -fovHalf, fovHalf, false);
    coneShape.lineTo(0, 0);
    this.fovConeGeom = new THREE.ShapeGeometry(coneShape);
    this.fovConeMat = new THREE.MeshBasicMaterial({
      color: 0x00ffcc,
      transparent: true,
      opacity: WebGLRenderer.FOV_CONE_OPACITY,
      depthTest: false,
      side: THREE.DoubleSide,
    });

    // 6.1: flock neighbor connection lines. One dynamic buffer; capacity grows
    // only when more segments than the current allocation are needed.
    this.neighborGeom = new THREE.BufferGeometry();
    this.neighborLines = new THREE.LineSegments(
      this.neighborGeom,
      new THREE.LineBasicMaterial({ color: 0x00ffcc, transparent: true, opacity: 0.35, depthTest: false }),
    );
    this.neighborLines.renderOrder = 6;
    this.scene.add(this.neighborLines);

    // 6.1: memory dots (remembered food) — instanced small dots, scale + color
    // fade with memory strength.
    const dotGeom = new THREE.CircleGeometry(0.08, 8);
    const dotMat = new THREE.MeshBasicMaterial({ color: 0xffffff, transparent: true, opacity: 0.9, depthTest: false });
    this.memoryDots = new THREE.InstancedMesh(dotGeom, dotMat, WebGLRenderer.MEMORY_DOT_CAPACITY);
    this.memoryDots.frustumCulled = false;
    this.memoryDots.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
    this.memoryDots.renderOrder = 7;
    this.scene.add(this.memoryDots);
  }

  // 6.4: zoom to a factor centered on `cx, cy` (world units), clamped.
  setZoom(zoom: number, cx?: number, cy?: number) {
    const z = Math.min(10, Math.max(1, zoom));
    if (typeof cx === 'number' && typeof cy === 'number') {
      this.viewCenter.x = cx;
      this.viewCenter.y = cy;
    }
    this.viewZoom = z;
    this.applyCamera();
  }

  // 6.4: pan the view center by a world-space delta (drag-to-pan).
  panBy(dx: number, dy: number) {
    this.viewCenter.x = Math.min(WebGLRenderer.WORLD_W, Math.max(0, this.viewCenter.x + dx));
    this.viewCenter.y = Math.min(WebGLRenderer.WORLD_H, Math.max(0, this.viewCenter.y + dy));
    this.applyCamera();
  }

  // 6.4: reset to the full-arena view.
  resetView() {
    this.viewZoom = 1;
    this.viewCenter.x = WebGLRenderer.WORLD_W / 2;
    this.viewCenter.y = WebGLRenderer.WORLD_H / 2;
    this.applyCamera();
  }

  // Audit 2 Task 2: toggle the flock/neighbor connection lines. When hidden,
  // `updateNeighborLines()` is also skipped entirely (no per-frame pair scan).
  setNeighborLinesVisible(visible: boolean) {
    this.neighborLinesVisible = visible;
    this.neighborLines.visible = visible;
  }

  // 6.4: convert a mouse pixel (relative to the canvas) to world coordinates.
  screenToWorld(px: number, py: number, canvasW: number, canvasH: number): { x: number; y: number } {
    const halfW = WebGLRenderer.WORLD_W / (2 * this.viewZoom);
    const halfH = WebGLRenderer.WORLD_H / (2 * this.viewZoom);
    const worldX = this.viewCenter.x + ((px / canvasW) - 0.5) * 2 * halfW;
    const worldY = this.viewCenter.y - ((py / canvasH) - 0.5) * 2 * halfH;
    return { x: worldX, y: worldY };
  }

  // 6.4: expose the current zoom factor (for pan-speed scaling in the app).
  viewZoomRef(): number {
    return this.viewZoom;
  }

  // Audit 4 §9.2: real paint fps (measured from the rAF clock in render()).
  paintFps(): number {
    return this.paintFpsValue;
  }

  private applyCamera() {
    const halfW = WebGLRenderer.WORLD_W / (2 * this.viewZoom);
    const halfH = WebGLRenderer.WORLD_H / (2 * this.viewZoom);
    this.camera.left = this.viewCenter.x - halfW;
    this.camera.right = this.viewCenter.x + halfW;
    this.camera.top = this.viewCenter.y + halfH;
    this.camera.bottom = this.viewCenter.y - halfH;
    this.camera.updateProjectionMatrix();
  }

  // 6.6: grow an instanced mesh's buffers when it needs more instances than
  // the initial capacity. Keeps `count` within capacity so Three never drops
  // instances silently at high agent counts.
  private ensureCapacity(mesh: THREE.InstancedMesh, needed: number) {
    if (needed <= mesh.count) return;
    const capacity = Math.max(needed, mesh.instanceMatrix.count * 2);
    const newMatrices = new THREE.InstancedBufferAttribute(new Float32Array(capacity * 16), 16);
    if (mesh.instanceMatrix.array) {
      (newMatrices.array as Float32Array).set(mesh.instanceMatrix.array as Float32Array);
    }
    mesh.instanceMatrix = newMatrices;
    mesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);
    // Instance colors (morph/FSM tints) must grow too or setColorAt is dropped.
    if (mesh.instanceColor) {
      const old = mesh.instanceColor as THREE.InstancedBufferAttribute;
      const newColors = new THREE.InstancedBufferAttribute(new Float32Array(capacity * 3), 3);
      (newColors.array as Float32Array).set(old.array as Float32Array);
      mesh.instanceColor = newColors;
    }
    mesh.count = capacity;
  }

  render(
    snapshot: any,
    previousSnapshot: any = null,
    lastReceivedAt: number = 0,
    currentReceivedAt: number = 0,
    selectedUids: string[] = [],
    hoveredUid?: string,
  ) {
    if (!snapshot || !snapshot.agents) return;

    // 6.5: advance the animation clock at real time (wings/peck pulses).
    const now = performance.now();
    this.animTime += Math.min(0.05, (now - this.lastFrameMs) / 1000);
    this.lastFrameMs = now;
    const t = this.animTime;

    // Audit 4 §9.2: paint-fps from this same per-frame delta (rolling 500 ms).
    this.paintFrames += 1;
    if (now - this.paintWindowStart >= 500) {
      this.paintFpsValue = Math.round((this.paintFrames * 1000) / (now - this.paintWindowStart));
      this.paintFrames = 0;
      this.paintWindowStart = now;
    }

    // Audit 3 Phase 4: network interpolation. Trailing the real-time clock by
    // one inter-arrival interval means `renderTime` always falls between the
    // two received snapshots, so entities glide continuously instead of
    // snapping to each WS packet. Clamped to [0,1] — never extrapolate past
    // the current snapshot (a dead/late network just holds the last pose).
    const inter = currentReceivedAt - lastReceivedAt;
    const alpha = previousSnapshot && inter > 0
      ? Math.min(1, Math.max(0, (now - lastReceivedAt - inter) / inter))
      : 1;

    // Audit 4 §9.1: the interpolation arrays/Map are only allocated while
    // actually interpolating (alpha < 1). In the steady state — a snapshot per
    // ~16 ms broadcast with alpha held at 1 between arrivals — we pass the raw
    // snapshot arrays straight through, so render() does zero per-frame
    // allocation on the agent/predator hot path (this was the dominant
    // client-side GC source during long sessions).
    let effectiveAgents: any[] = snapshot.agents;
    let effectivePredators: any[] = snapshot.predators || [];
    if (previousSnapshot && alpha < 1) {
      const prevAgents = new Map<string, any>();
      for (const a of previousSnapshot.agents || []) prevAgents.set(a.uid, a);
      effectiveAgents = snapshot.agents.map((a: any) => {
        const p = prevAgents.get(a.uid);
        return p ? WebGLRenderer.resolveAgent(p, a, alpha) : a;
      });
      const prevPreds = new Map<string, any>();
      for (const pp of previousSnapshot.predators || []) prevPreds.set(pp.uid, pp);
      effectivePredators = (snapshot.predators || []).map((pp: any) => {
        const p = prevPreds.get(pp.uid);
        return p ? WebGLRenderer.resolveAgent(p, pp, alpha) : pp;
      });
    }

    // 6.6: grow instance capacity once (if the population exceeded the pool).
    const n = snapshot.agents.length;
    this.ensureCapacity(this.birdShadowMesh, n);
    this.ensureCapacity(this.birdBodyMesh, n);
    this.ensureCapacity(this.birdHeadMesh, n);
    this.ensureCapacity(this.birdWingLMesh, n);
    this.ensureCapacity(this.birdWingRMesh, n);
    this.ensureCapacity(this.stateRingMesh, n);
    this.ensureCapacity(this.grainMesh, snapshot.grains ? snapshot.grains.length : 0);

    const dummyBody = this.dummyBody;
    const dummyHead = this.dummyHead;
    const dummyWingL = this.dummyWingL;
    const dummyWingR = this.dummyWingR;
    const dummyShadow = this.dummyShadow;
    const dummyRing = this.dummyRing;
    const dummyGrain = this.dummyGrain;
    const dummyPred = this.dummyPred;
    const bodyColor = this.scratchColor;

    // 6.5: nearest-grain distance per agent (for the peck trigger).
    const grains = snapshot.grains || [];

    effectiveAgents.forEach((agent: any, i: number) => {
      const angle = agent.heading;
      const mass = agent.mass_g ? agent.mass_g / 315.0 : 1.0;
      const bob = agent.head_offset || [0, 0];
      // 6.6: dirty-flag — static bones (body/shadow/ring) skip matrix + color
      // recompute when nothing moved/changed. Wings + head are always updated
      // because they carry the time-based flap/peck animation.
      const fsm = agent.fsm_state || 'Idle';
      const cached = this.agentCache.get(agent.uid);
      const dirty = !cached
        || cached.pos[0] !== agent.pos[0] || cached.pos[1] !== agent.pos[1]
        || cached.heading !== angle || cached.mass !== mass
        || cached.bob[0] !== bob[0] || cached.bob[1] !== bob[1]
        || cached.fsm !== fsm;
      if (dirty) {
        this.agentCache.set(agent.uid, { pos: [agent.pos[0], agent.pos[1]], heading: angle, mass, bob: [bob[0], bob[1]], fsm });

        // Shadow under the bird (fixed size scaled by mass).
        dummyShadow.position.set(agent.pos[0], agent.pos[1], 0.02);
        dummyShadow.scale.set(mass, mass, 1);
        dummyShadow.updateMatrix();
        this.birdShadowMesh.setMatrixAt(i, dummyShadow.matrix);

        // Teardrop body, rotated to heading, scaled by mass.
        dummyBody.position.set(agent.pos[0], agent.pos[1], 0.08);
        dummyBody.rotation.z = angle;
        dummyBody.scale.set(1.0 * mass, 1.0 * mass, 1.0);
        dummyBody.updateMatrix();
        this.birdBodyMesh.setMatrixAt(i, dummyBody.matrix);
        // 6.5: per-instance morph color (gray/brown/white from UID hash).
        const hash = agent.uid.split('').reduce((acc: number, c: string) => acc + c.charCodeAt(0), 0);
        bodyColor.setHex(MORPH_COLORS[hash % MORPH_COLORS.length]);
        this.birdBodyMesh.setColorAt(i, bodyColor);

        // 6.5: FSM state ring under the bird.
        dummyRing.position.set(agent.pos[0], agent.pos[1], 0.04);
        dummyRing.scale.set(mass, mass, 1);
        dummyRing.updateMatrix();
        this.stateRingMesh.setMatrixAt(i, dummyRing.matrix);
        this.stateRingMesh.setColorAt(i, bodyColor.setHex(FSM_COLORS[fsm] ?? 0x888888));
      }

      const v = agent.vel ? Math.hypot(agent.vel[0], agent.vel[1]) : 0;

      // 6.5: wings — flap sweeps fore/aft, amplitude scales with speed (tucked
      // when idle). Left wing flaps opposite the right.
      const flapAmp = v > WebGLRenderer.FLAP_MIN_SPEED_MS
        ? Math.min(0.55, 0.15 + v * 0.25)
        : 0.03;
      const flapFreq = WebGLRenderer.FLAP_FREQ_BASE + v * WebGLRenderer.FLAP_FREQ_PER_SPEED;
      const flapPhase = Math.sin(t * flapFreq * Math.PI * 2) * flapAmp;

      const cosA = Math.cos(angle);
      const sinA = Math.sin(angle);
      // Left wing pivots at (0.05, +0.12) rotated by heading; sweep by -flapPhase.
      const lx = agent.pos[0] + cosA * 0.05 - sinA * 0.12;
      const ly = agent.pos[1] + sinA * 0.05 + cosA * 0.12;
      dummyWingL.position.set(lx, ly, 0.1);
      dummyWingL.rotation.z = angle - flapPhase;
      dummyWingL.scale.set(mass, mass, 1);
      dummyWingL.updateMatrix();
      this.birdWingLMesh.setMatrixAt(i, dummyWingL.matrix);

      const rx = agent.pos[0] + cosA * 0.05 + sinA * 0.12;
      const ry = agent.pos[1] + sinA * 0.05 - cosA * 0.12;
      dummyWingR.position.set(rx, ry, 0.1);
      dummyWingR.rotation.z = angle + flapPhase;
      dummyWingR.scale.set(mass, mass, 1);
      dummyWingR.updateMatrix();
      this.birdWingRMesh.setMatrixAt(i, dummyWingR.matrix);

      // 6.5: head — forward offset + head-bob offset + peck thrust. Peck reuses
      // the HeadBobSystem jerk curve (10t³-15t⁴+6t⁵) as a longer forward thrust
      // when foraging near a grain.
      let headFwd = 0.45;
      let peckThrust = 0;
      if (fsm === 'Foraging' && Array.isArray(grains) && grains.length > 0) {
        let nearest = Infinity;
        for (const g of grains) {
          const dx = g[0] - agent.pos[0];
          const dy = g[1] - agent.pos[1];
          const d2 = dx * dx + dy * dy;
          if (d2 < nearest) nearest = d2;
        }
        if (nearest < WebGLRenderer.PECK_NEAR_GRAIN_M * WebGLRenderer.PECK_NEAR_GRAIN_M) {
          const pt = (t * WebGLRenderer.PECK_FREQ_HZ) % 1.0;
          const jerk = 10 * pt ** 3 - 15 * pt ** 4 + 6 * pt ** 5;
          peckThrust = WebGLRenderer.PECK_EXTRA_M * jerk;
        }
      }
      headFwd += peckThrust;
      const headX = agent.pos[0] + cosA * headFwd + bob[0];
      const headY = agent.pos[1] + sinA * headFwd + bob[1];
      dummyHead.position.set(headX, headY, 0.18);
      dummyHead.rotation.z = angle;
      dummyHead.scale.set(1.2 * mass, 1.2 * mass, 1);
      dummyHead.updateMatrix();
      this.birdHeadMesh.setMatrixAt(i, dummyHead.matrix);
    });

    // 6.6: prune cache entries for agents that left the sim (stale UIDs).
    if (this.agentCache.size > n) {
      const live = new Set(effectiveAgents.map((a: any) => a.uid));
      for (const uid of this.agentCache.keys()) {
        if (!live.has(uid)) this.agentCache.delete(uid);
      }
    }

    this.birdBodyMesh.count = n;
    this.birdHeadMesh.count = n;
    this.birdWingLMesh.count = n;
    this.birdWingRMesh.count = n;
    this.birdShadowMesh.count = n;
    this.stateRingMesh.count = n;
    this.birdBodyMesh.instanceMatrix.needsUpdate = true;
    this.birdHeadMesh.instanceMatrix.needsUpdate = true;
    this.birdWingLMesh.instanceMatrix.needsUpdate = true;
    this.birdWingRMesh.instanceMatrix.needsUpdate = true;
    this.birdShadowMesh.instanceMatrix.needsUpdate = true;
    this.stateRingMesh.instanceMatrix.needsUpdate = true;
    if (this.birdBodyMesh.instanceColor) this.birdBodyMesh.instanceColor.needsUpdate = true;
    if (this.stateRingMesh.instanceColor) this.stateRingMesh.instanceColor.needsUpdate = true;

    if (snapshot.grains) {
      const grains = snapshot.grains;
      const prevGrains = this.lastGrains;
      grains.forEach((g: any, i: number) => {
        // 6.6 + Audit 4 §9.1: index-based dirty check (grains are a
        // position-ordered Vec, never reorder) — replaces the per-frame
        // `toFixed(4)` string-key allocations and the unbounded grainCache.
        if (prevGrains && prevGrains[i] && prevGrains[i][0] === g[0] && prevGrains[i][1] === g[1]) return;
        dummyGrain.position.set(g[0], g[1], 0);
        dummyGrain.scale.set(1, 1, 1);
        dummyGrain.updateMatrix();
        this.grainMesh.setMatrixAt(i, dummyGrain.matrix);
      });
      this.lastGrains = grains;
      this.grainMesh.count = grains.length;
      this.grainMesh.instanceMatrix.needsUpdate = true;
    }

    if (snapshot.predators) {
      const prevPredsArr = this.lastPreds;
      effectivePredators.forEach((p: any, i: number) => {
        // 6.6 + Audit 4 §9.1: index-based dirty check — replaces the never-
        // pruned uid-keyed predCache (which grew unbounded as predators died
        // and new uids spawned over a long session).
        if (prevPredsArr && prevPredsArr[i] && prevPredsArr[i].pos[0] === p.pos[0] && prevPredsArr[i].pos[1] === p.pos[1]) return;
        dummyPred.position.set(p.pos[0], p.pos[1], 0.2);
        dummyPred.scale.set(1, 1, 1);
        dummyPred.updateMatrix();
        this.predatorMesh.setMatrixAt(i, dummyPred.matrix);
      });
      this.lastPreds = effectivePredators;
      this.predatorMesh.count = snapshot.predators.length;
      this.predatorMesh.instanceMatrix.needsUpdate = true;
    }

    // 2.2b: countdown label (remaining seconds, small font) beside each predator.
    this.updatePredatorLabels(effectivePredators);

    // 4.3: draw the static urban obstacles.
    this.updateObstacles(snapshot.obstacles || []);

    // 6.1: flock neighbor connection lines between nearby agents. Audit 2
    // Task 2: skip the O(n²) pair scan entirely when the lines are hidden.
    if (this.neighborLinesVisible) {
      this.updateNeighborLines(effectiveAgents);
    }

    // 6.1: memory dots — remembered food as fading dots per agent.
    this.updateMemoryDots(effectiveAgents);

    // Marking tool: green ring around every selected agent/predator.
    this.updateSelectionMarkers(effectiveAgents, effectivePredators, selectedUids);

    // 6.1: FOV cone for the hovered + selected agents (agent head direction).
    this.updateFovCones(effectiveAgents, selectedUids, hoveredUid);

    // 4.4: tint the scene by weather (scaled by the smooth transition intensity).
    const weatherOpacity = this.updateWeatherOverlay(snapshot.weather, snapshot.weather_intensity);

    // 2.3 + Audit 4 §9.4: dim toward night — light_level 1.0 (noon) → 0,
    // 0.1 (darkest) → 0.9, scaled to a 0.65 ceiling. Combined with any weather
    // tint the total is capped at 0.75 so the scene NEVER approaches full
    // black (light_level floors at 0.1 but birds/dashboard must stay legible
    // through the darkest rainy night).
    if (typeof snapshot.light_level === 'number') {
      const mat = this.nightOverlay.material as THREE.MeshBasicMaterial;
      const nightDim = Math.max(0, 1 - snapshot.light_level) * 0.65;
      mat.opacity = Math.max(0, Math.min(nightDim, 0.75 - weatherOpacity));
    }

    this.renderer.render(this.scene, this.camera);
  }

  // 6.1: draw line segments between every agent pair within the flock radius.
  // O(n²) at 30 agents is trivial (≤ 435 pairs); fine for the research view.
  private updateNeighborLines(agents: any[]) {
    const pairs: number[] = [];
    const r = WebGLRenderer.FLOCK_LINE_RADIUS;
    for (let i = 0; i < agents.length; i++) {
      const a = agents[i];
      for (let j = i + 1; j < agents.length; j++) {
        const b = agents[j];
        const dx = a.pos[0] - b.pos[0];
        const dy = a.pos[1] - b.pos[1];
        if (dx * dx + dy * dy <= r * r) {
          pairs.push(a.pos[0], a.pos[1], 0.25, b.pos[0], b.pos[1], 0.25);
        }
      }
    }
    const count = pairs.length / 3;
    let attr = this.neighborGeom.getAttribute('position') as THREE.BufferAttribute;
    if (!attr || attr.count < count) {
      const capacity = Math.max(count * 3, 3);
      attr = new THREE.BufferAttribute(new Float32Array(capacity), 3);
      this.neighborGeom.setAttribute('position', attr);
    }
    if (count > 0) {
      (attr.array as Float32Array).set(pairs);
    }
    attr.needsUpdate = true;
    this.neighborGeom.setDrawRange(0, count);
    this.neighborLines.frustumCulled = false;
  }

  // 6.1: remembered food dots. Each agent's `memory` is [[x, y, strength]];
  // scale + brightness fade with strength so aging memories dim to nothing.
  private updateMemoryDots(agents: any[]) {
    const dummy = this.dummyShadow; // 6.6: reuse a pooled dummy (zero allocs).
    const dotColor = this.scratchColor;
    let index = 0;
    agents.forEach((a) => {
      if (!Array.isArray(a.memory)) return;
      a.memory.forEach((slot: [number, number, number]) => {
        const strength = Math.max(0, Math.min(1, slot[2]));
        if (strength <= 0.02) return;
        dummy.position.set(slot[0], slot[1], 0.3);
        const s = 0.6 + 1.4 * strength;
        dummy.scale.set(s, s, s);
        dummy.updateMatrix();
        this.memoryDots.setMatrixAt(index, dummy.matrix);
        dotColor.setRGB(1.0, 0.42 + 0.4 * strength, 0.05 * strength);
        this.memoryDots.setColorAt(index, dotColor);
        index++;
      });
    });
    this.memoryDots.count = index;
    this.memoryDots.instanceMatrix.needsUpdate = true;
    if (this.memoryDots.instanceColor) this.memoryDots.instanceColor.needsUpdate = true;
  }

  // 6.1: FOV cone for the hovered agent + every selected agent. The cone is a
  // 340°-wide wedge (pigeon vision) drawn as a flat fan on the ground plane,
  // rotated to the agent's heading.
  private updateFovCones(agents: any[], selectedUids: string[], hoveredUid?: string) {
    const targets = new Set<string>();
    if (hoveredUid) targets.add(hoveredUid);
    selectedUids.forEach((u) => targets.add(u));

    const coneAgents = agents.filter((a) => targets.has(a.uid));

    while (this.fovCones.length < coneAgents.length) {
      const mesh = new THREE.Mesh(this.fovConeGeom, this.fovConeMat);
      mesh.renderOrder = 6;
      this.scene.add(mesh);
      this.fovCones.push(mesh);
    }
    for (let i = 0; i < this.fovCones.length; i++) {
      const mesh = this.fovCones[i];
      if (i < coneAgents.length) {
        const a = coneAgents[i];
        mesh.visible = true;
        mesh.position.set(a.pos[0], a.pos[1], 0.28);
        mesh.rotation.z = a.heading;
      } else {
        mesh.visible = false;
      }
    }
  }

  // Audit 4 §9.4: returns the applied weather-tint opacity so the caller can
  // cap the COMBINED night+weather darkness (never full black).
  private updateWeatherOverlay(weather: string, intensity: number): number {
    const i = Math.min(1, Math.max(0, intensity ?? 0));
    const mat = this.weatherOverlay.material as THREE.MeshBasicMaterial;
    let opacity = 0;
    if (weather === 'Rain') {
      mat.color.setHex(0x2f6fdb);
      opacity = 0.35 * i;
    } else if (weather === 'Heat') {
      mat.color.setHex(0xff9a3c);
      opacity = 0.25 * i;
    } else {
      mat.color.setHex(0xffffff);
    }
    mat.opacity = opacity;
    return opacity;
  }

  // 4.3: rebuild the static obstacle quads from the snapshot each frame.
  // Obstacles are few (≤ a handful), so full rebuild is cheaper than a
  // diffed pool and avoids any stale-geometry state.
  private updateObstacles(
    obstacles: Array<{ id: number; kind: string; min: [number, number]; max: [number, number] }>,
  ) {
    for (const child of this.obstacleGroup.children) {
      this.obstacleGroup.remove(child);
      (child as THREE.Mesh).geometry.dispose();
      ((child as THREE.Mesh).material as THREE.Material).dispose();
    }

    const kindColor: Record<string, number> = {
      Building: 0x8b93a6,
      Wall: 0x5a6472,
      Water: 0x3b82f6,
      Tree: 0x22c55e,
    };
    obstacles.forEach((o) => {
      const w = o.max[0] - o.min[0];
      const h = o.max[1] - o.min[1];
      const geom = new THREE.PlaneGeometry(w, h);
      const mat = new THREE.MeshBasicMaterial({
        color: kindColor[o.kind] ?? 0x888888,
        transparent: o.kind === 'Water',
        opacity: o.kind === 'Water' ? 0.75 : 1.0,
        depthTest: false,
      });
      const mesh = new THREE.Mesh(geom, mat);
      mesh.position.set((o.min[0] + o.max[0]) / 2, (o.min[1] + o.max[1]) / 2, 0.15);
      mesh.renderOrder = 5;
      this.obstacleGroup.add(mesh);
    });
  }

  private updatePredatorLabels(predators: Array<{ uid?: string; pos: [number, number]; lifetime_remaining_s: number }>) {
    const live = new Set<string>();
    predators.forEach((p) => {
      const uid = p.uid ?? `${p.pos[0]},${p.pos[1]}`;
      live.add(uid);
      const seconds = Math.max(0, p.lifetime_remaining_s ?? 0).toFixed(1);

      // Audit 2 Task 4: reuse the cached sprite if the predator already has a
      // label — only rebuild the canvas/texture when the value actually changed.
      let cached = this.predatorLabels.get(uid);
      if (!cached) {
        const canvas = document.createElement('canvas');
        canvas.width = 96;
        canvas.height = 40;
        const texture = new THREE.CanvasTexture(canvas);
        const mat = new THREE.SpriteMaterial({ map: texture, transparent: true, depthTest: false });
        const sprite = new THREE.Sprite(mat);
        sprite.scale.set(1.1, 0.46, 1);
        sprite.renderOrder = 1000;
        this.scene.add(sprite);
        cached = { sprite, text: '', texture };
        this.predatorLabels.set(uid, cached);
      }
      cached.sprite.position.set(p.pos[0] + 0.9, p.pos[1] + 0.9, 1);

      if (cached.text !== seconds) {
        const g = cached.texture.image.getContext('2d')!;
        g.clearRect(0, 0, 96, 40);
        g.font = 'bold 28px monospace';
        g.textAlign = 'center';
        g.textBaseline = 'middle';
        g.fillStyle = '#000';
        g.fillText(seconds, 48, 20);
        g.fillStyle = '#ffffff';
        g.fillText(seconds, 47, 19);
        cached.texture.needsUpdate = true;
        cached.text = seconds;
      }
    });

    // Prune labels for predators that left the sim.
    for (const [uid, entry] of this.predatorLabels) {
      if (!live.has(uid)) {
        this.scene.remove(entry.sprite);
        entry.texture.dispose();
        (entry.sprite.material as THREE.Material).dispose();
        this.predatorLabels.delete(uid);
      }
    }
  }

  private updateSelectionMarkers(agents: any[], predators: any[], selectedUids: string[]) {
    for (const m of this.selectionMarkers) {
      this.scene.remove(m);
      m.geometry.dispose();
      (m.material as THREE.Material).dispose();
    }
    this.selectionMarkers = [];
    if (selectedUids.length === 0) return;

    const selected = new Map<string, [number, number]>();
    (agents || []).forEach((a: any) => { if (selectedUids.includes(a.uid)) selected.set(a.uid, a.pos); });
    (predators || []).forEach((p: any) => { if (selectedUids.includes(p.uid)) selected.set(p.uid, p.pos); });

    const ringGeom = new THREE.RingGeometry(0.62, 0.72, 24);
    const ringMat = new THREE.MeshBasicMaterial({ color: 0x00ffcc, transparent: true, opacity: 0.9, depthTest: false });
    selected.forEach((pos) => {
      const ring = new THREE.Mesh(ringGeom, ringMat);
      ring.position.set(pos[0], pos[1], 0.6);
      ring.renderOrder = 999;
      this.scene.add(ring);
      this.selectionMarkers.push(ring);
    });
  }
}
