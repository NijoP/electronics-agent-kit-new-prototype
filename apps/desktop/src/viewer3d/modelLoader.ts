// Loading and caching of attached 3D model files (render-independent of placement).
//
// * Everything heavy is imported lazily: occt-import-js (WASM, STEP), VRMLLoader and GLTFLoader are
//   only fetched the first time a model of that format is actually needed. A project without
//   models never loads them.
// * The cache is keyed by the model *file* (format + path), never by the component transform or the
//   project revision, so unrelated project edits do not reload models. A failure is cached too (no
//   retry storm); `evictModel` forces a retry.
// * Loaded objects are in MODEL space, millimetres, Z up (see modelTransform.ts). Placement is the
//   viewer's job. Cached geometry is shared between instances and must not be disposed per rebuild.
import * as THREE from "three";
import type { ModelRef } from "../types";

export type ReadModelBytes = (path: string) => Promise<ArrayBuffer>;

export const modelKey = (m: ModelRef) => `${m.format}:${m.source.path}`;

const cache = new Map<string, Promise<THREE.Object3D>>();

/** Typed failure so the UI can show *why* a model is missing. */
export class ModelLoadError extends Error {}

let occt: Promise<import("occt-import-js").Occt> | null = null;
function occtModule() {
  occt ??= Promise.all([import("occt-import-js"), import("occt-import-js/dist/occt-import-js.wasm?url")]).then(([m, w]) =>
    m.default({ locateFile: (name: string) => (name.endsWith(".wasm") ? w.default : name) }),
  );
  return occt;
}

async function loadStep(bytes: ArrayBuffer): Promise<THREE.Object3D> {
  const o = await occtModule();
  const r = o.ReadStepFile(new Uint8Array(bytes), null);
  if (!r.success || r.meshes.length === 0) throw new ModelLoadError("STEP file has no readable geometry");
  const root = new THREE.Group();
  for (const m of r.meshes) {
    const g = new THREE.BufferGeometry();
    g.setAttribute("position", new THREE.Float32BufferAttribute(Array.from(m.attributes.position.array), 3));
    if (m.attributes.normal) g.setAttribute("normal", new THREE.Float32BufferAttribute(Array.from(m.attributes.normal.array), 3));
    g.setIndex(Array.from(m.index.array));
    if (!m.attributes.normal) g.computeVertexNormals();
    const [cr, cg, cb] = m.color ?? [0.6, 0.6, 0.62];
    root.add(new THREE.Mesh(g, new THREE.MeshStandardMaterial({ color: new THREE.Color(cr, cg, cb), roughness: 0.6, metalness: 0.2 })));
  }
  return root;
}

async function loadWrl(bytes: ArrayBuffer): Promise<THREE.Object3D> {
  const { VRMLLoader } = await import("three/examples/jsm/loaders/VRMLLoader.js");
  // VRML units are whatever the file says (KiCad .wrl: 1 unit = 0.1 inch); the component's
  // ModelTransform.scale is where the user corrects that.
  return new VRMLLoader().parse(new TextDecoder().decode(bytes), "");
}

async function loadGlb(bytes: ArrayBuffer): Promise<THREE.Object3D> {
  const { GLTFLoader } = await import("three/examples/jsm/loaders/GLTFLoader.js");
  const gltf = await new GLTFLoader().parseAsync(bytes, "");
  // glTF is metres, Y up; the model frame is millimetres, Z up.
  const root = new THREE.Group();
  gltf.scene.rotation.x = Math.PI / 2;
  root.add(gltf.scene);
  root.scale.setScalar(1000);
  return root;
}

/** Load (or reuse) the model file for `ref`. Rejects with `ModelLoadError` / the read error. */
export function loadModel(ref: ModelRef, read: ReadModelBytes): Promise<THREE.Object3D> {
  const key = modelKey(ref);
  let p = cache.get(key);
  if (!p) {
    p = read(ref.source.path).then((bytes) => (ref.format === "step" ? loadStep(bytes) : ref.format === "wrl" ? loadWrl(bytes) : loadGlb(bytes)));
    cache.set(key, p);
  }
  return p;
}

/** Forget one cached model (for example after the user replaced the file) so it is re-read. */
export function evictModel(ref: ModelRef) {
  cache.delete(modelKey(ref));
}

/** Test hook: number of cached models. */
export const cachedModelCount = () => cache.size;
export function clearModelCache() {
  cache.clear();
}
