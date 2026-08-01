import * as THREE from 'three';

export class WebGLRenderer {
  private scene: THREE.Scene;
  private camera: THREE.OrthographicCamera;
  private renderer: THREE.WebGLRenderer;
  private birdBodyMesh: THREE.InstancedMesh;
  private birdHeadMesh: THREE.InstancedMesh;
  private grainMesh: THREE.InstancedMesh;
  
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
  }

  render(snapshot: any) {
    if (!snapshot || !snapshot.agents) return;
    
    const dummyBody = new THREE.Object3D();
    const dummyHead = new THREE.Object3D();
    const dummyGrain = new THREE.Object3D();
    
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
        dummyGrain.position.set(g[0][0], g[0][1], 0);
        dummyGrain.scale.set(1, 1, 1);
        dummyGrain.updateMatrix();
        this.grainMesh.setMatrixAt(i, dummyGrain.matrix);
      });
      this.grainMesh.count = snapshot.grains.length;
      this.grainMesh.instanceMatrix.needsUpdate = true;
    }
    
    this.renderer.render(this.scene, this.camera);
  }
}