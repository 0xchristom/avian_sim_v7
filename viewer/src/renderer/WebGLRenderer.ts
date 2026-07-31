import * as THREE from 'three';

export class WebGLRenderer {
  private scene: THREE.Scene;
  private camera: THREE.OrthographicCamera;
  private renderer: THREE.WebGLRenderer;
  private birdMesh: THREE.InstancedMesh;
  
  constructor(canvas: HTMLCanvasElement) {
    this.scene = new THREE.Scene();
    this.camera = new THREE.OrthographicCamera(0, 32, 0, 21, 0.1, 1000);
    this.camera.position.z = 10;
    
    this.renderer = new THREE.WebGLRenderer({ canvas, antialias: true });
    this.renderer.setSize(800, 533);
    
    const geometry = new THREE.CircleGeometry(0.4, 16);
    const material = new THREE.MeshBasicMaterial({ color: 0x888888 });
    this.birdMesh = new THREE.InstancedMesh(geometry, material, 1000);
    this.scene.add(this.birdMesh);
  }

  render(snapshot: any) {
    if (!snapshot) return;
    
    const dummy = new THREE.Object3D();
    snapshot.agents.forEach((agent: any, i: number) => {
      dummy.position.set(agent.pos[0], agent.pos[1], 0);
      dummy.rotation.z = agent.heading;
      dummy.updateMatrix();
      this.birdMesh.setMatrixAt(i, dummy.matrix);
    });
    this.birdMesh.count = snapshot.agents.length;
    this.birdMesh.instanceMatrix.needsUpdate = true;
    
    this.renderer.render(this.scene, this.camera);
  }
}
