import * as THREE from 'three';

export class WebGLRenderer {
  private scene: THREE.Scene;
  private camera: THREE.OrthographicCamera;
  private renderer: THREE.WebGLRenderer;
  private birdBodyMesh: THREE.InstancedMesh;
  private birdHeadMesh: THREE.InstancedMesh;
  private grainMesh: THREE.InstancedMesh;
  private predatorMesh: THREE.InstancedMesh;
  private nightOverlay: THREE.Mesh;
  private obstacleGroup: THREE.Group;
  private predatorLabels: THREE.Sprite[] = [];
  private selectionMarkers: THREE.Mesh[] = [];

  constructor(canvas: HTMLCanvasElement) {
    this.scene = new THREE.Scene();
    this.camera = new THREE.OrthographicCamera(0, 32, 21, 0, 0.1, 1000);
    this.camera.position.z = 10;

    this.renderer = new THREE.WebGLRenderer({ canvas, antialias: true });
    this.renderer.setSize(800, 533);
    this.renderer.setClearColor(0x1a1c25);

    const grid = new THREE.GridHelper(64, 64, 0x333333, 0x222222);
    grid.rotation.x = Math.PI / 2;
    grid.position.set(16, 10.5, -1);
    this.scene.add(grid);

    const bodyGeom = new THREE.CircleGeometry(0.4, 16);
    const bodyMat = new THREE.MeshBasicMaterial({ color: 0xff6c0c });
    this.birdBodyMesh = new THREE.InstancedMesh(bodyGeom, bodyMat, 1000);
    this.birdBodyMesh.frustumCulled = false;
    this.scene.add(this.birdBodyMesh);

    const headGeom = new THREE.CircleGeometry(0.15, 16);
    const headMat = new THREE.MeshBasicMaterial({ color: 0xffffff });
    this.birdHeadMesh = new THREE.InstancedMesh(headGeom, headMat, 1000);
    this.birdHeadMesh.frustumCulled = false;
    this.scene.add(this.birdHeadMesh);

    const grainGeom = new THREE.CircleGeometry(0.2, 8);
    const grainMat = new THREE.MeshBasicMaterial({ color: 0xffff00 });
    this.grainMesh = new THREE.InstancedMesh(grainGeom, grainMat, 1000);
    this.grainMesh.frustumCulled = false;
    this.scene.add(this.grainMesh);

    // 2.2: predators rendered as larger red triangles (hawk).
    const predGeom = new THREE.CircleGeometry(0.55, 16);
    const predMat = new THREE.MeshBasicMaterial({ color: 0xff2222 });
    this.predatorMesh = new THREE.InstancedMesh(predGeom, predMat, 64);
    this.predatorMesh.frustumCulled = false;
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

    // 4.3: static urban obstacles (buildings/trees/water), rebuilt per frame.
    this.obstacleGroup = new THREE.Group();
    this.scene.add(this.obstacleGroup);
  }

  render(snapshot: any, selectedUids: string[] = []) {
    if (!snapshot || !snapshot.agents) return;

    const dummyBody = new THREE.Object3D();
    const dummyHead = new THREE.Object3D();
    const dummyGrain = new THREE.Object3D();
    const dummyPred = new THREE.Object3D();

    snapshot.agents.forEach((agent: any, i: number) => {
      const angle = agent.heading;
      const massScale = agent.mass_g ? agent.mass_g / 315.0 : 1.0;

      dummyBody.position.set(agent.pos[0], agent.pos[1], 0);
      dummyBody.rotation.z = angle;
      dummyBody.scale.set(1.0 * massScale, 0.6 * massScale, 1.0);
      dummyBody.updateMatrix();
      this.birdBodyMesh.setMatrixAt(i, dummyBody.matrix);

      const headX = agent.pos[0] + Math.cos(angle) * 0.45 + (agent.head_offset ? agent.head_offset[0] : 0);
      const headY = agent.pos[1] + Math.sin(angle) * 0.45 + (agent.head_offset ? agent.head_offset[1] : 0);
      dummyHead.position.set(headX, headY, 0.1);
      dummyHead.rotation.z = angle;
      dummyHead.scale.set(1, 1, 1);
      dummyHead.updateMatrix();
      this.birdHeadMesh.setMatrixAt(i, dummyHead.matrix);
    });

    this.birdBodyMesh.count = snapshot.agents.length;
    this.birdHeadMesh.count = snapshot.agents.length;
    this.birdBodyMesh.instanceMatrix.needsUpdate = true;
    this.birdHeadMesh.instanceMatrix.needsUpdate = true;

    if (snapshot.grains) {
      snapshot.grains.forEach((g: any, i: number) => {
        dummyGrain.position.set(g[0], g[1], 0);
        dummyGrain.scale.set(1, 1, 1);
        dummyGrain.updateMatrix();
        this.grainMesh.setMatrixAt(i, dummyGrain.matrix);
      });
      this.grainMesh.count = snapshot.grains.length;
      this.grainMesh.instanceMatrix.needsUpdate = true;
    }

    if (snapshot.predators) {
      snapshot.predators.forEach((p: any, i: number) => {
        dummyPred.position.set(p.pos[0], p.pos[1], 0.2);
        dummyPred.scale.set(1, 1, 1);
        dummyPred.updateMatrix();
        this.predatorMesh.setMatrixAt(i, dummyPred.matrix);
      });
      this.predatorMesh.count = snapshot.predators.length;
      this.predatorMesh.instanceMatrix.needsUpdate = true;
    }

    // 2.2b: countdown label (remaining seconds, small font) beside each predator.
    this.updatePredatorLabels(snapshot.predators || []);

    // 4.3: draw the static urban obstacles.
    this.updateObstacles(snapshot.obstacles || []);

    // Marking tool: green ring around every selected agent/predator.
    this.updateSelectionMarkers(snapshot, selectedUids);

    // 2.3: dim toward night — light_level 1.0 (noon) → overlay 0, 0.1 → 0.9.
    if (typeof snapshot.light_level === 'number') {
      const mat = this.nightOverlay.material as THREE.MeshBasicMaterial;
      mat.opacity = Math.max(0, 1 - snapshot.light_level);
    }

    this.renderer.render(this.scene, this.camera);
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

  private updatePredatorLabels(predators: Array<{ pos: [number, number]; lifetime_remaining_s: number }>) {
    for (const label of this.predatorLabels) {
      this.scene.remove(label);
      label.material.map?.dispose();
      label.material.dispose();
    }
    this.predatorLabels = [];

    predators.forEach((p) => {
      const seconds = Math.max(0, p.lifetime_remaining_s ?? 0).toFixed(1);
      const canvas = document.createElement('canvas');
      canvas.width = 96;
      canvas.height = 40;
      const g = canvas.getContext('2d')!;
      g.clearRect(0, 0, canvas.width, canvas.height);
      g.font = 'bold 28px monospace';
      g.textAlign = 'center';
      g.textBaseline = 'middle';
      g.fillStyle = '#000';
      g.fillText(seconds, 48, 20);
      g.fillStyle = '#ffffff';
      g.fillText(seconds, 47, 19);

      const texture = new THREE.CanvasTexture(canvas);
      const mat = new THREE.SpriteMaterial({ map: texture, transparent: true, depthTest: false });
      const sprite = new THREE.Sprite(mat);
      sprite.position.set(p.pos[0] + 0.9, p.pos[1] + 0.9, 1);
      sprite.scale.set(1.1, 0.46, 1);
      sprite.renderOrder = 1000;
      this.scene.add(sprite);
      this.predatorLabels.push(sprite);
    });
  }

  private updateSelectionMarkers(snapshot: any, selectedUids: string[]) {
    for (const m of this.selectionMarkers) {
      this.scene.remove(m);
      m.geometry.dispose();
      (m.material as THREE.Material).dispose();
    }
    this.selectionMarkers = [];
    if (selectedUids.length === 0) return;

    const selected = new Map<string, [number, number]>();
    (snapshot.agents || []).forEach((a: any) => { if (selectedUids.includes(a.uid)) selected.set(a.uid, a.pos); });
    (snapshot.predators || []).forEach((p: any) => { if (selectedUids.includes(p.uid)) selected.set(p.uid, p.pos); });

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
