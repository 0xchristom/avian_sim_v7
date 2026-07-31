import * as THREE from 'three';

export class WebGLRenderer {
    private scene: THREE.Scene;
    private camera: THREE.OrthographicCamera;
    private renderer: THREE.WebGLRenderer;
    private birdBodyMesh: THREE.InstancedMesh;
    private birdHeadMesh: THREE.InstancedMesh;
    private bodyColors: Float32Array;
    private headColors: Float32Array;

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

        // Ciało z instanced color attribute
        const bodyGeom = new THREE.CircleGeometry(0.35, 16);
        const bodyMat = new THREE.MeshBasicMaterial({ color: 0xffffff, vertexColors: true });
        this.birdBodyMesh = new THREE.InstancedMesh(bodyGeom, bodyMat, 1000);
        this.scene.add(this.birdBodyMesh);
        this.bodyColors = new Float32Array(1000 * 3);

        const headGeom = new THREE.CircleGeometry(0.15, 12);
        const headMat = new THREE.MeshBasicMaterial({ color: 0xffffff, vertexColors: true });
        this.birdHeadMesh = new THREE.InstancedMesh(headGeom, headMat, 1000);
        this.scene.add(this.birdHeadMesh);
        this.headColors = new Float32Array(1000 * 3);
    }

    private getAgentColor(age: number, hunger: number, energy: number): THREE.Color {
        const color = new THREE.Color();
        if (age > 10.0) {
            color.setHex(0xcc8866); // starszy ptak — brązowawy
        } else if (age < 1.0) {
            color.setHex(0x88ccff); // młody — niebieskawy
        } else {
            color.setHex(0x888888); // dorosły — szary
        }
        
        // Modyfikacja wg głodu i energii
        if (hunger > 0.8) {
            color.lerp(new THREE.Color(0xff4444), 0.5); // czerwony gdy głodny
        } else if (energy < 10.0) {
            color.lerp(new THREE.Color(0x4444ff), 0.3); // niebieski gdy wyczerpany
        }
        return color;
    }

    render(snapshot: any) {
        if (!snapshot) return;

        const dummyBody = new THREE.Object3D();
        const dummyHead = new THREE.Object3D();

        snapshot.agents.forEach((agent: any, i: number) => {
            const angle = agent.heading;
            const color = this.getAgentColor(agent.age_years, agent.hunger, agent.energy_kj);

            // Skalowanie ciała wg masy (300g = 1.0, zakres 0.7-1.3)
            const massScale = 0.7 + (agent.mass_g / 300.0) * 0.6;
            
            dummyBody.position.set(agent.pos[0], agent.pos[1], 0);
            dummyBody.rotation.z = angle;
            dummyBody.scale.set(massScale, massScale * 0.6, 1.0);
            dummyBody.updateMatrix();
            this.birdBodyMesh.setMatrixAt(i, dummyBody.matrix);
            
            this.bodyColors[i * 3] = color.r;
            this.bodyColors[i * 3 + 1] = color.g;
            this.bodyColors[i * 3 + 2] = color.b;

            const headOffsetX = Math.cos(angle) * 0.4 * massScale;
            const headOffsetY = Math.sin(angle) * 0.4 * massScale;
            dummyHead.position.set(agent.pos[0] + headOffsetX, agent.pos[1] + headOffsetY, 0.1);
            dummyHead.rotation.z = angle;
            dummyHead.scale.set(1, 1, 1);
            dummyHead.updateMatrix();
            this.birdHeadMesh.setMatrixAt(i, dummyHead.matrix);
            
            this.headColors[i * 3] = color.r + 0.1;
            this.headColors[i * 3 + 1] = color.g + 0.1;
            this.headColors[i * 3 + 2] = color.b + 0.1;
        });

        this.birdBodyMesh.count = snapshot.agents.length;
        this.birdHeadMesh.count = snapshot.agents.length;
        this.birdBodyMesh.instanceMatrix.needsUpdate = true;
        this.birdHeadMesh.instanceMatrix.needsUpdate = true;
        
        // Aktualizacja kolorów
        (this.birdBodyMesh.geometry as any).setAttribute('color', new THREE.InstancedBufferAttribute(this.bodyColors.slice(0, snapshot.agents.length * 3), 3));
        
        this.renderer.render(this.scene, this.camera);
    }
}