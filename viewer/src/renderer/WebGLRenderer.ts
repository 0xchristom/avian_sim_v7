import * as THREE from 'three';

export class WebGLRenderer {
  private scene: THREE.Scene;
  private camera: THREE.OrthographicCamera;
  private renderer: THREE.WebGLRenderer;
  private birdBodyMesh: THREE.InstancedMesh;
  private birdHeadMesh: THREE.InstancedMesh;
  
  constructor(canvas: HTMLCanvasElement) {
    this.scene = new THREE.Scene();
    this.camera = new THREE.OrthographicCamera(0, 32, 21, 0, 0.1, 1000);
    this.camera.position.z = 10;
    
    this.renderer = new THREE.WebGLRenderer({ canvas, antialias: true });
    this.renderer.setSize(800, 533);
    this.renderer.setClearColor(0x1a1c25);
    
    // Siatka tła
    const grid = new THREE.GridHelper(64, 64, 0x333333, 0x222222);
    grid.rotation.x = Math.PI / 2;
    grid.position.set(16, 10.5, -1);
    this.scene.add(grid);

    // Ciało gołębia (szara elipsa)
    const bodyGeom = new THREE.CircleGeometry(0.4, 16);
    const bodyMat = new THREE.MeshBasicMaterial({ color: 0x888888 });
    this.birdBodyMesh = new THREE.InstancedMesh(bodyGeom, bodyMat, 1000);
    this.scene.add(this.birdBodyMesh);

    // Głowa gołębia (jaśniejszy okrąg)
    const headGeom = new THREE.CircleGeometry(0.15, 16);
    const headMat = new THREE.MeshBasicMaterial({ color: 0xaaaaaa });
    this.birdHeadMesh = new THREE.InstancedMesh(headGeom, headMat, 1000);
    this.scene.add(this.birdHeadMesh);
  }

  render(snapshot: any) {
    if (!snapshot) return;
    
    const dummyBody = new THREE.Object3D();
    const dummyHead = new THREE.Object3D();
    
    snapshot.agents.forEach((agent: any, i: number) => {
      const angle = agent.heading;
      
      // Ustawienie tułowia (skalowanie X tworzy elipsę w osi ruchu)
      dummyBody.position.set(agent.pos[0], agent.pos[1], 0);
      dummyBody.rotation.z = angle;
      dummyBody.scale.set(1.0, 0.6, 1.0); 
      dummyBody.updateMatrix();
      this.birdBodyMesh.setMatrixAt(i, dummyBody.matrix);
      
      // Ustawienie głowy (przesunięcie w kierunku patrzenia o 0.45 jednostki)
      const headOffsetX = Math.cos(angle) * 0.45;
      const headOffsetY = Math.sin(angle) * 0.45;
      dummyHead.position.set(agent.pos[0] + headOffsetX, agent.pos[1] + headOffsetY, 0.1);
      dummyHead.rotation.z = angle;
      dummyHead.scale.set(1, 1, 1);
      dummyHead.updateMatrix();
      this.birdHeadMesh.setMatrixAt(i, dummyHead.matrix);
    });
    
    this.birdBodyMesh.count = snapshot.agents.length;
    this.birdHeadMesh.count = snapshot.agents.length;
    this.birdBodyMesh.instanceMatrix.needsUpdate = true;
    this.birdHeadMesh.instanceMatrix.needsUpdate = true;
    
    this.renderer.render(this.scene, this.camera);
  }
}