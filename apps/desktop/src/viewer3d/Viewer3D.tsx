// The 3D document (three.js, slices C6 and E6): board outline extruded at the stack-up thickness
// in the project's solder-mask colour, copper layers in the PCB layer manager's colours, pads in the
// surface-finish colour, tracks, vias, courtyard outlines, real STEP/WRL/GLB models where attached
// and labelled placeholders where not. Orbit / pan / zoom, perspective or orthographic projection,
// seven view presets, reset, a measure tool and cross-probing into the editors.
// It re-reads the engine snapshot whenever the project revision changes, so edits made anywhere
// (UI, CLI, AI) show up here. Edits made from here (board colour and finish, model assignment) are
// typed engine commands (`SetManufacturing`, `UpsertComponentDefinition`). Geometry is derived
// data, not electrical truth.
import { useEffect, useMemo, useRef, useState } from "react";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { CSS2DObject, CSS2DRenderer } from "three/examples/jsm/renderers/CSS2DRenderer.js";
import { api, pickModelFile } from "../api";
import { layerColour, stackLayers } from "../layers";
import { modelFormOf, modelRefFromForm, QUALITIES, type ModelForm } from "../shell/LibraryPanel";
import type { Command, ComponentDef, CopperLayer, ModelQuality, ModelRef, Snapshot } from "../types";
import { buildScene, modelStatus, type BoardScene, type ModelStatus } from "./scene";
import { evictModel, loadModel, modelKey } from "./modelLoader";
import { modelToWorld } from "./modelTransform";
import { publishView3d } from "./controls";
import { appearance, FINISHES, MASK_COLOURS, measure, measureText, orthoHalfHeight, placeholderReason, presetPose, PRESETS, stackTable, type Projection, type View3dPreset } from "./view";
import { onProfile, profileNow, type ResourceProfile } from "../resourceProfile";

interface Props {
  projectId: string | null;
  revision: number | null;
  active: boolean;
  /** Engine symbol id selected in the editors (highlighted here: cross-probe in). */
  selected?: string | null;
  /** Select an engine symbol id in the editors (cross-probe out). */
  onSelect?: (symbolId: string) => void;
  /** Open the PCB or the schematic on the selection. */
  onProbe?: (kind: "pcb" | "schematic") => void;
  /** Runs a typed engine command through the workspace (absent: the view is read-only). */
  execute?: (c: Command) => void | Promise<void>;
  busy?: boolean;
  /** The PCB layer manager's colour per copper layer (layer colour sync). */
  layerColourOf?: (l: CopperLayer) => string;
}

/** Layer-group keys beyond the copper layers. */
const GROUPS = [
  { key: "board", label: "Board" },
  { key: "models", label: "3D models" },
  { key: "bodies-top", label: "Top bodies" },
  { key: "bodies-bottom", label: "Bottom bodies" },
  { key: "outline-top", label: "Top courtyards" },
  { key: "outline-bottom", label: "Bottom courtyards" },
  { key: "vias", label: "Vias" },
] as const;

const BOARD_OPACITY = 0.72;
const BODY = 0x3a3f47;
/** Placeholder edges: a part drawn without a loaded model is never a silent box. */
const PLACEHOLDER = 0xe0a030;
const HIGHLIGHT = 0x4fc3f7;
const FOV = 40;

function disposeGroup(g: THREE.Object3D) {
  g.traverse((o) => {
    const m = o as THREE.Mesh;
    m.geometry?.dispose?.();
    const mat = m.material as THREE.Material | THREE.Material[] | undefined;
    if (Array.isArray(mat)) mat.forEach((x) => x.dispose());
    else mat?.dispose?.();
  });
}

const tmp = new THREE.Object3D();
/** One InstancedMesh from a list of [x, y, z, sx, sy, sz, rotY] boxes/cylinders (world units mm). */
function instanced(geo: THREE.BufferGeometry, color: number | string, items: number[][]): THREE.InstancedMesh | null {
  if (items.length === 0) return null;
  const mesh = new THREE.InstancedMesh(geo, new THREE.MeshStandardMaterial({ color, roughness: 0.55, metalness: 0.25 }), items.length);
  items.forEach(([x, y, z, sx, sy, sz, ry], i) => {
    tmp.position.set(x, y, z);
    tmp.rotation.set(0, ry, 0);
    tmp.scale.set(sx, sy, sz);
    tmp.updateMatrix();
    mesh.setMatrixAt(i, tmp.matrix);
  });
  mesh.instanceMatrix.needsUpdate = true;
  return mesh;
}

const BOX_EDGES = [[0, 1], [1, 2], [2, 3], [3, 0], [4, 5], [5, 6], [6, 7], [7, 4], [0, 4], [1, 5], [2, 6], [3, 7]];
/** The 12 edges of an axis-aligned box, appended as line-segment coordinates. */
function boxEdges(out: number[], x0: number, y0: number, z0: number, x1: number, y1: number, z1: number) {
  const c = [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1], [x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]];
  for (const [a, b] of BOX_EDGES) out.push(...c[a], ...c[b]);
}

/** Surface height (world Y) of a board side. */
const surfaceOf = (sc: BoardScene, side: "top" | "bottom") => {
  const l = side === "top" ? sc.layers[0] : sc.layers[sc.layers.length - 1];
  return side === "top" ? l.y + l.thickness / 2 : l.y - l.thickness / 2;
};

/** World-space box [x0, y0, z0, x1, y1, z1] of a part: its body box, else a thin slab on its courtyard. */
function partBox(sc: BoardScene, footprintId: string): number[] | null {
  const i = sc.parts.findIndex((p) => p.footprintId === footprintId);
  const part = sc.parts[i];
  const cy = sc.courtyards[i];
  if (!part || !cy) return null;
  const body = sc.bodies.find((b) => b.footprintId === footprintId);
  const s = surfaceOf(sc, part.side);
  const top = s + (part.side === "top" ? 1 : -1) * (body?.height ?? 0.2);
  return [cy.x - cy.hw, Math.min(s, top), cy.y - cy.hh, cy.x + cy.hw, Math.max(s, top), cy.y + cy.hh];
}

export interface Look {
  mask: number;
  pad: number;
}

/** Build the three.js groups, keyed by visibility group. PCB (x, y) maps to world (X, Z); Y is up. */
export function buildGroups(sc: BoardScene, modelled: ReadonlySet<string> = new Set(), look: Look = { mask: MASK_COLOURS.Green, pad: FINISHES.ENIG }): Map<string, THREE.Group> {
  const groups = new Map<string, THREE.Group>();
  const group = (key: string) => {
    let g = groups.get(key);
    if (!g) {
      g = new THREE.Group();
      g.name = key;
      groups.set(key, g);
    }
    return g;
  };
  const box = new THREE.BoxGeometry(1, 1, 1);
  const cyl = new THREE.CylinderGeometry(0.5, 0.5, 1, 24);
  const t = sc.thickness;

  if (sc.outline.length >= 3) {
    const shape = new THREE.Shape(sc.outline.map(([x, y]) => new THREE.Vector2(x, y)));
    const geo = new THREE.ExtrudeGeometry(shape, { depth: t, bevelEnabled: false });
    geo.rotateX(Math.PI / 2); // shape (x, y) -> world (x, -depth, y)
    geo.translate(0, t / 2, 0);
    const mesh = new THREE.Mesh(geo, new THREE.MeshStandardMaterial({ color: look.mask, transparent: true, opacity: BOARD_OPACITY, roughness: 0.8, side: THREE.DoubleSide }));
    mesh.name = "board-slab";
    group("board").add(mesh);
  }

  for (const l of sc.layers) {
    const g = group(`cu:${l.key}`);
    const tracks = sc.tracks
      .filter((tr) => tr.layer === l.key)
      .map((tr) => {
        const dx = tr.bx - tr.ax;
        const dz = tr.by - tr.ay;
        const len = Math.hypot(dx, dz) + tr.width;
        return [(tr.ax + tr.bx) / 2, l.y, (tr.ay + tr.by) / 2, len, l.thickness, tr.width, -Math.atan2(dz, dx)];
      });
    const boxPads = sc.pads.filter((p) => p.layer === l.key && p.shape !== "circle").map((p) => [p.x, l.y + 0.002, p.y, p.sx, l.thickness, p.sy, 0]);
    const roundPads = sc.pads.filter((p) => p.layer === l.key && p.shape === "circle").map((p) => [p.x, l.y + 0.002, p.y, p.sx, l.thickness, p.sy, 0]);
    for (const m of [instanced(box, l.colour, tracks), instanced(box, look.pad, boxPads), instanced(cyl, look.pad, roundPads)]) if (m) g.add(m);
    for (const z of sc.zones.filter((zz) => zz.layer === l.key)) {
      const pts = z.outline.map(([x, y]) => new THREE.Vector3(x, l.y + l.thickness, y));
      if (pts.length > 1) g.add(new THREE.LineLoop(new THREE.BufferGeometry().setFromPoints(pts), new THREE.LineBasicMaterial({ color: l.colour })));
    }
  }

  const vias = sc.vias.map((v) => [v.x, (v.yTop + v.yBottom) / 2, v.y, v.diameter, v.yTop - v.yBottom, v.diameter, 0]);
  const viaMesh = instanced(cyl, look.pad, vias);
  if (viaMesh) group("vias").add(viaMesh);

  for (const side of ["top", "bottom"] as const) {
    const edge = surfaceOf(sc, side);
    const dir = side === "top" ? 1 : -1;
    const sideBodies = sc.bodies.filter((b) => b.side === side && !modelled.has(b.footprintId));
    const bodies = sideBodies.map((b) => [b.x, edge + (dir * b.height) / 2, b.y, b.sx, b.height, b.sy, 0]);
    const m = instanced(box, BODY, bodies);
    if (m) {
      m.userData.ids = sideBodies.map((b) => b.footprintId);
      group(`bodies-${side}`).add(m);
      // Placeholder edges: these boxes stand in for a model that is missing or did not load.
      const seg: number[] = [];
      for (const b of sideBodies) boxEdges(seg, b.x - b.sx / 2, edge, b.y - b.sy / 2, b.x + b.sx / 2, edge + dir * b.height, b.y + b.sy / 2);
      const g = new THREE.BufferGeometry();
      g.setAttribute("position", new THREE.Float32BufferAttribute(seg, 3));
      group(`bodies-${side}`).add(new THREE.LineSegments(g, new THREE.LineBasicMaterial({ color: PLACEHOLDER })));
    }
    // Courtyard rectangles stand in for silkscreen outlines (the IR has no silkscreen artwork yet).
    const seg: number[] = [];
    for (const c of sc.courtyards.filter((cy) => cy.side === side)) {
      const y = edge + dir * 0.01;
      const x0 = c.x - c.hw, x1 = c.x + c.hw, z0 = c.y - c.hh, z1 = c.y + c.hh;
      seg.push(x0, y, z0, x1, y, z0, x1, y, z0, x1, y, z1, x1, y, z1, x0, y, z1, x0, y, z1, x0, y, z0);
    }
    if (seg.length) {
      const g = new THREE.BufferGeometry();
      g.setAttribute("position", new THREE.Float32BufferAttribute(seg, 3));
      group(`outline-${side}`).add(new THREE.LineSegments(g, new THREE.LineBasicMaterial({ color: 0xe8e8e8 })));
    }
  }
  return groups;
}

const STATUS_TEXT: Record<ModelStatus, string> = {
  VENDOR_EXACT: "Vendor exact",
  LIBRARY_VERIFIED: "Library verified",
  DERIVED_VALIDATED: "Derived, validated",
  UNCLASSIFIED: "Model attached, quality not declared",
  MISSING: "Missing",
};

/** Footprint id of whatever a ray hit: a placed model or an instanced box body. */
function hitFootprint(h: THREE.Intersection): string | null {
  for (let o: THREE.Object3D | null = h.object; o; o = o.parent) {
    if (typeof o.userData.footprintId === "string") return o.userData.footprintId;
    const ids = o.userData.ids as string[] | undefined;
    if (ids && h.instanceId != null) return ids[h.instanceId] ?? null;
  }
  return null;
}

type V3 = [number, number, number];
type Load = { ok: boolean; error?: string };
interface Three {
  persp: THREE.PerspectiveCamera;
  ortho: THREE.OrthographicCamera;
  /** Orthographic frustum half height before zoom. */
  orthoHalf: number;
  camera: () => THREE.PerspectiveCamera | THREE.OrthographicCamera;
  controls: OrbitControls;
  render: () => void;
  resize: () => void;
  content: THREE.Group;
  /** Placed real models. Instances share the cache's geometry, so this group is never disposed per rebuild. */
  models: THREE.Group;
  labels: THREE.Group;
  overlay: THREE.Group;
  groups: Map<string, THREE.Group>;
}

export function Viewer3D({ projectId, revision, active, selected = null, onSelect, onProbe, execute, busy = false, layerColourOf = layerColour }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const three = useRef<Three | null>(null);
  const [loads, setLoads] = useState<Record<string, Load>>({});
  const [reloadNonce, setReloadNonce] = useState(0);
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [webgl, setWebgl] = useState(true);
  const [gpuLost, setGpuLost] = useState(false);
  const [ready, setReady] = useState(false);
  const [hidden, setHidden] = useState<Set<string>>(new Set());
  const [projection, setProjectionState] = useState<Projection>("perspective");
  const [measuring, setMeasuring] = useState(false);
  const [points, setPoints] = useState<V3[]>([]);
  const [labelsOn, setLabelsOn] = useState(true);
  const hiddenRef = useRef(hidden);
  hiddenRef.current = hidden;
  const measuringRef = useRef(measuring);
  measuringRef.current = measuring;
  const fitted = useRef<string | null>(null);
  const pick = useRef<(id: string) => void>(() => undefined);

  // Layer colour sync: the scene is rebuilt when the PCB layer manager's colours change.
  const colourSig = snapshot ? stackLayers(snapshot.project.pcb.board.stackup).map((l) => layerColourOf(l.layer)).join(",") : "";
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const scene = useMemo(() => (snapshot ? buildScene(snapshot, layerColourOf) : null), [snapshot, colourSig]);
  const look = useMemo(() => (snapshot ? appearance(snapshot) : null), [snapshot]);
  const stack = useMemo(() => (snapshot ? stackTable(snapshot) : null), [snapshot]);
  /** Footprint ids whose real model loaded (comma-joined so it works as an effect dependency). */
  const modelled = useMemo(
    () =>
      Object.entries(loads)
        .filter(([, v]) => v.ok)
        .map(([k]) => k)
        .sort()
        .join(","),
    [loads],
  );
  const selectedFp = useMemo(() => (snapshot && selected ? (Object.values(snapshot.project.pcb.footprints).find((f) => f.symbol_id === selected)?.id ?? null) : null), [snapshot, selected]);

  // 3D click -> the engine symbol of that footprint, selected in the editors.
  pick.current = (fpId: string) => {
    const sym = snapshot?.project.pcb.footprints[fpId]?.symbol_id;
    if (sym) onSelect?.(sym);
  };

  // Live update: re-read the active project's snapshot when it changes (only while this view is shown).
  useEffect(() => {
    if (!active || !projectId) return;
    let cancelled = false;
    api
      .currentProject()
      .then((s) => {
        if (cancelled) return;
        setSnapshot(s);
        setError(null);
      })
      .catch((e) => {
        if (!cancelled) setError(typeof e === "string" ? e : ((e as { message?: string })?.message ?? "Could not read the project"));
      });
    return () => {
      cancelled = true;
    };
  }, [active, projectId, revision]);

  // Renderer, cameras and controls: created once per mount.
  useEffect(() => {
    const el = host.current;
    if (!el) return;
    let renderer: THREE.WebGLRenderer;
    // Quality tier from the resource profile: antialiasing is fixed per renderer, the back-buffer
    // pixel ratio follows profile changes live. Canvas size and geometry are never scaled.
    const tier = profileNow();
    try {
      renderer = new THREE.WebGLRenderer({ antialias: tier.antialias });
    } catch {
      setWebgl(false);
      return;
    }
    const pixelRatio = (p: ResourceProfile) => Math.min(window.devicePixelRatio || 1, Math.max(p.pixel_ratio_cap, 0.5));
    renderer.setPixelRatio(pixelRatio(tier));
    renderer.setClearColor(0x14181d);
    el.appendChild(renderer.domElement);
    renderer.domElement.style.display = "block";
    // Placeholder labels are DOM text over the canvas (readable, and visible to UI tests).
    const labelRenderer = new CSS2DRenderer();
    labelRenderer.domElement.className = "v3d-labels";
    el.appendChild(labelRenderer.domElement);
    const sc = new THREE.Scene();
    const persp = new THREE.PerspectiveCamera(FOV, 1, 0.1, 5000);
    persp.position.set(0, 80, 60);
    const ortho = new THREE.OrthographicCamera(-1, 1, 1, -1, 0.1, 5000);
    sc.add(new THREE.HemisphereLight(0xffffff, 0x303844, 1.1));
    const sun = new THREE.DirectionalLight(0xffffff, 1.4);
    sun.position.set(40, 120, 60);
    sc.add(sun);
    const fill = new THREE.DirectionalLight(0xffffff, 0.6);
    fill.position.set(-40, -120, -60);
    sc.add(fill);
    const content = new THREE.Group();
    const models = new THREE.Group();
    const labels = new THREE.Group();
    const overlay = new THREE.Group();
    sc.add(content, models, labels, overlay);
    const controls = new OrbitControls(persp, renderer.domElement);
    controls.enableDamping = false;
    controls.screenSpacePanning = true;
    controls.maxPolarAngle = Math.PI;
    const camera = () => controls.object as THREE.PerspectiveCamera | THREE.OrthographicCamera;

    // Demand-driven: one frame per change (camera, resize, rebuild, layers, models), no loop.
    let queued = false;
    let contextLost = false;
    let frame = 0;
    const render = () => {
      if (queued || contextLost) return;
      queued = true;
      frame = requestAnimationFrame(() => {
        queued = false;
        if (contextLost) return;
        renderer.render(sc, camera());
        labelRenderer.render(sc, camera());
      });
    };
    controls.addEventListener("change", render);
    const state: Three = { persp, ortho, orthoHalf: 50, camera, controls, render, resize: () => undefined, content, models, labels, overlay, groups: new Map() };
    const resize = () => {
      const w = Math.max(el.clientWidth, 1);
      const h = Math.max(el.clientHeight, 1);
      renderer.setSize(w, h, false);
      renderer.domElement.style.width = `${w}px`;
      renderer.domElement.style.height = `${h}px`;
      labelRenderer.setSize(w, h);
      persp.aspect = w / h;
      persp.updateProjectionMatrix();
      const half = state.orthoHalf;
      ortho.left = -half * (w / h);
      ortho.right = half * (w / h);
      ortho.top = half;
      ortho.bottom = -half;
      ortho.updateProjectionMatrix();
      render();
    };
    state.resize = resize;
    const ro = new ResizeObserver(resize);
    ro.observe(el);
    resize();
    // Click (not drag): in measure mode it picks a point; otherwise it selects the part under the
    // cursor (cross-probe to the editors).
    const ray = new THREE.Raycaster();
    let down: [number, number] | null = null;
    const dom = renderer.domElement;
    const onDown = (e: PointerEvent) => {
      down = [e.clientX, e.clientY];
    };
    const onUp = (e: PointerEvent) => {
      if (!down || Math.hypot(e.clientX - down[0], e.clientY - down[1]) > 4) return;
      const r = dom.getBoundingClientRect();
      ray.setFromCamera(new THREE.Vector2(((e.clientX - r.left) / r.width) * 2 - 1, -((e.clientY - r.top) / r.height) * 2 + 1), camera());
      const hits = ray.intersectObjects([models, content], true).filter((h) => (h.object as THREE.Mesh).isMesh);
      if (measuringRef.current) {
        const h = hits[0];
        if (h) setPoints((p) => [...(p.length >= 2 ? [] : p), [h.point.x, h.point.y, h.point.z] as V3]);
        return;
      }
      for (const h of hits) {
        const id = hitFootprint(h);
        if (id) {
          pick.current(id);
          return;
        }
      }
    };
    dom.addEventListener("pointerdown", onDown);
    dom.addEventListener("pointerup", onUp);
    // GPU reset or resource exhaustion: keep the editors usable, say what happened, and redraw
    // once the browser restores the context (three.js re-uploads buffers on the next render).
    const onLost = (e: Event) => {
      e.preventDefault(); // allow the context to be restored
      contextLost = true;
      setGpuLost(true);
    };
    const onRestored = () => {
      contextLost = false;
      setGpuLost(false);
      render();
    };
    dom.addEventListener("webglcontextlost", onLost);
    dom.addEventListener("webglcontextrestored", onRestored);
    const offProfile = onProfile((p) => {
      renderer.setPixelRatio(pixelRatio(p));
      resize();
    });
    three.current = state;
    setReady(true);
    return () => {
      offProfile();
      cancelAnimationFrame(frame);
      dom.removeEventListener("pointerdown", onDown);
      dom.removeEventListener("pointerup", onUp);
      dom.removeEventListener("webglcontextlost", onLost);
      dom.removeEventListener("webglcontextrestored", onRestored);
      models.clear();
      labels.clear();
      ro.disconnect();
      controls.dispose();
      disposeGroup(content);
      disposeGroup(overlay);
      renderer.dispose();
      // Release the GL context now instead of waiting for GC: WebViews cap live contexts.
      renderer.forceContextLoss();
      renderer.domElement.remove();
      labelRenderer.domElement.remove();
      three.current = null;
      setReady(false);
    };
  }, []);

  /** Points the active camera from a preset, fitted to the board. */
  const preset = (p: View3dPreset, sc: BoardScene | null = scene) => {
    const t = three.current;
    if (!t || !sc) return;
    const pose = presetPose(p, sc.bounds, FOV);
    const cam = t.camera();
    t.controls.target.set(...pose.target);
    cam.position.set(...pose.position);
    cam.near = Math.max(pose.distance / 500, 0.05);
    cam.far = pose.distance * 20;
    if (cam === t.ortho) {
      t.orthoHalf = orthoHalfHeight(pose.distance, FOV);
      t.ortho.zoom = 1;
      t.resize();
    }
    cam.updateProjectionMatrix();
    t.controls.update();
    t.render();
  };

  /** Switches projection, keeping the viewpoint (the ortho frustum matches the perspective view). */
  const setProjection = (p: Projection) => {
    const t = three.current;
    if (!t) return;
    const from = t.camera();
    const to = p === "orthographic" ? t.ortho : t.persp;
    if (from !== to) {
      to.position.copy(from.position);
      to.quaternion.copy(from.quaternion);
      const d = from.position.distanceTo(t.controls.target);
      to.near = Math.max(d / 500, 0.05);
      to.far = d * 20;
      if (to === t.ortho) {
        t.orthoHalf = orthoHalfHeight(d, FOV);
        t.ortho.zoom = 1;
      }
      t.controls.object = to;
      t.resize();
      to.updateProjectionMatrix();
      t.controls.update();
    }
    setProjectionState(p);
    t.render();
  };

  const zoomBy = (f: number) => {
    const t = three.current;
    if (!t) return;
    const cam = t.camera();
    if (cam === t.ortho) {
      t.ortho.zoom = Math.min(Math.max(t.ortho.zoom / f, 0.05), 200);
      t.ortho.updateProjectionMatrix();
    } else {
      const off = cam.position.clone().sub(t.controls.target).multiplyScalar(f);
      cam.position.copy(t.controls.target).add(off);
    }
    t.controls.update();
    t.render();
  };

  const reset = () => {
    setPoints([]);
    setMeasuring(false);
    // The camera switch is synchronous, so the preset then fits the perspective camera.
    if (projection !== "perspective") setProjection("perspective");
    preset("iso");
  };

  const reloadModels = () => {
    for (const p of scene?.parts ?? []) if (p.model) evictModel(p.model);
    setLoads({});
    setReloadNonce((n) => n + 1);
  };

  // Rebuild geometry when the scene data, appearance or loaded models change; keep the user's
  // camera except on the first load.
  useEffect(() => {
    const t = three.current;
    if (!t || !scene || !look) return;
    t.content.clear();
    t.groups.forEach(disposeGroup);
    // A box is replaced only by a model that actually loaded; otherwise the placeholder stays.
    const groups = buildGroups(scene, new Set(modelled.split(",").filter(Boolean)), look);
    groups.forEach((g, k) => {
      g.visible = !hiddenRef.current.has(k);
      t.content.add(g);
    });
    t.groups = groups;
    if (fitted.current !== projectId) {
      fitted.current = projectId;
      preset("iso", scene);
    } else t.render();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scene, ready, modelled, look]);

  // Placeholder labels: every part without a loaded model says so on the board (never a silent box).
  useEffect(() => {
    const t = three.current;
    if (!t || !scene) return;
    t.labels.clear();
    scene.parts.forEach((p) => {
      const why = placeholderReason(p.model != null, loads[p.footprintId], scene.bodies.some((b) => b.footprintId === p.footprintId));
      const box = why ? partBox(scene, p.footprintId) : null;
      if (!why || !box) return;
      const div = document.createElement("div");
      div.className = "v3d-ph";
      div.dataset.part = p.refdes;
      div.textContent = `${p.refdes} · ${why}`;
      const o = new CSS2DObject(div);
      o.position.set((box[0] + box[3]) / 2, p.side === "top" ? box[4] : box[1], (box[2] + box[5]) / 2);
      t.labels.add(o);
    });
    t.labels.visible = labelsOn;
    t.render();
  }, [scene, loads, labelsOn, ready]);

  // Overlay: the selection highlight (cross-probe in) and the measurement.
  useEffect(() => {
    const t = three.current;
    if (!t || !scene) return;
    disposeGroup(t.overlay);
    t.overlay.clear();
    const box = selectedFp ? partBox(scene, selectedFp) : null;
    if (box) {
      const seg: number[] = [];
      const m = 0.15;
      boxEdges(seg, box[0] - m, box[1] - m, box[2] - m, box[3] + m, box[4] + m, box[5] + m);
      const g = new THREE.BufferGeometry();
      g.setAttribute("position", new THREE.Float32BufferAttribute(seg, 3));
      t.overlay.add(new THREE.LineSegments(g, new THREE.LineBasicMaterial({ color: HIGHLIGHT, depthTest: false })));
    }
    const r = Math.max((scene.bounds[2] - scene.bounds[0]) / 300, 0.08);
    for (const p of points) {
      const s = new THREE.Mesh(new THREE.SphereGeometry(r, 12, 8), new THREE.MeshBasicMaterial({ color: HIGHLIGHT, depthTest: false }));
      s.position.set(...p);
      t.overlay.add(s);
    }
    if (points.length === 2) {
      const g = new THREE.BufferGeometry().setFromPoints(points.map((p) => new THREE.Vector3(...p)));
      t.overlay.add(new THREE.Line(g, new THREE.LineBasicMaterial({ color: HIGHLIGHT, depthTest: false })));
    }
    t.render();
  }, [scene, selectedFp, points, ready]);

  // Real 3D models: loaded lazily through the cache (keyed by model file, not by revision or
  // transform), then placed with the one ECAD -> viewer transform. Instances are cheap clones.
  useEffect(() => {
    const t = three.current;
    if (!t || !scene) return;
    let cancelled = false;
    t.models.clear();
    const results: Record<string, Load> = {};
    void Promise.all(
      scene.parts
        .filter((p) => p.model)
        .map(async (p) => {
          const ref = p.model!;
          try {
            const template = await loadModel(ref, api.libraryModelRead);
            if (cancelled) return;
            const obj = template.clone(true);
            obj.matrixAutoUpdate = false;
            obj.matrix.fromArray(modelToWorld(ref.transform, { x: p.x, y: p.y, rotationDeg: p.rotationDeg, side: p.side, surfaceY: surfaceOf(scene, p.side) }));
            obj.userData.footprintId = p.footprintId;
            t.models.add(obj);
            results[p.footprintId] = { ok: true };
          } catch (e) {
            results[p.footprintId] = { ok: false, error: e instanceof Error ? e.message : String(e) };
          }
        }),
    ).then(() => {
      if (cancelled) return;
      t.models.visible = !hiddenRef.current.has("models");
      setLoads(results);
      t.render();
    });
    return () => {
      cancelled = true;
    };
  }, [scene, ready, reloadNonce]);

  // Layer visibility.
  useEffect(() => {
    const t = three.current;
    if (!t) return;
    t.groups.forEach((g, k) => {
      g.visible = !hidden.has(k);
    });
    t.models.visible = !hidden.has("models");
    t.render();
  }, [hidden]);

  // Becoming visible again after being hidden: the canvas was 0-sized, so redraw.
  useEffect(() => {
    if (active) three.current?.render();
  }, [active]);

  // Publish the controls for the 3D document's menus, palette and toolbar (A1). Re-published when
  // the state they read or show changes (not on every render: publishing re-renders the workspace,
  // which re-renders this view); withdrawn on unmount. Without WebGL nothing is published and the
  // 3D actions say why they are unavailable.
  const modelCount = scene?.parts.filter((p) => p.model).length ?? 0;
  useEffect(() => {
    if (!ready) return;
    publishView3d({
      projection,
      measuring,
      measured: points.length > 0,
      labels: labelsOn,
      modelCount,
      preset: (p) => preset(p),
      reset,
      zoomBy,
      setProjection,
      toggleMeasure: () => setMeasuring((m) => !m),
      clearMeasure: () => setPoints([]),
      toggleLabels: () => setLabelsOn((l) => !l),
      reloadModels,
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ready, scene, projection, measuring, points.length, labelsOn, modelCount]);
  useEffect(() => () => publishView3d(null), []);

  const toggle = (key: string) =>
    setHidden((h) => {
      const n = new Set(h);
      if (n.has(key)) n.delete(key);
      else n.add(key);
      return n;
    });

  const toggles: { key: string; label: string; colour?: string }[] = [
    ...GROUPS.map((g) => ({ key: g.key as string, label: g.label as string })),
    ...(scene?.layers ?? []).map((l) => ({ key: `cu:${l.key}`, label: `${l.name} copper`, colour: l.colour })),
  ];
  const mfg = snapshot?.project.manufacturing;
  const mfgWhy = !execute ? "Unavailable: this view has no engine connection" : !mfg ? "Unavailable: the project has no manufacturing settings" : busy ? "An engine command is running" : null;
  const setMfg = (patch: { solder_mask_color?: string; surface_finish?: string }) => {
    if (mfg) void execute?.({ type: "SetManufacturing", config: { ...mfg, ...patch } });
  };
  const measured = points.length === 2 ? measureText(measure(points[0], points[1])) : null;
  const status = measured
    ? `Distance ${measured}`
    : measuring
      ? points.length === 1
        ? "Measure: click the second point"
        : "Measure: click the first point"
      : "Left-drag orbit · right-drag pan · wheel zoom · click a part to select it";
  const selPart = scene?.parts.find((p) => p.footprintId === selectedFp) ?? null;
  // Without WebGL the camera and measure controls cannot do anything: they say so instead.
  const noGl = webgl ? null : "Unavailable: WebGL is not available in this window";

  return (
    <div className="viewer3d" aria-label="3D board viewer" data-projection={projection}>
      <div className="v3d-bar" role="toolbar" aria-label="3D view">
        {PRESETS.map((p) => (
          <button key={p.id} type="button" disabled={noGl !== null} onClick={() => preset(p.id)} title={noGl ?? p.title}>
            {p.label}
          </button>
        ))}
        <button type="button" onClick={() => zoomBy(1 / 1.25)} disabled={noGl !== null} title={noGl ?? "Zoom in (or use the wheel)"} aria-label="Zoom in">
          +
        </button>
        <button type="button" onClick={() => zoomBy(1.25)} disabled={noGl !== null} title={noGl ?? "Zoom out (or use the wheel)"} aria-label="Zoom out">
          −
        </button>
        <button type="button" aria-pressed={projection === "orthographic"} onClick={() => setProjection(projection === "orthographic" ? "perspective" : "orthographic")} disabled={noGl !== null} title={noGl ?? "Switch between perspective and orthographic projection"}>
          Orthographic
        </button>
        <button type="button" disabled={noGl !== null} onClick={reset} title={noGl ?? "Isometric, perspective and fit to the board; clears any measurement"}>
          Reset
        </button>
        <button type="button" aria-pressed={measuring} onClick={() => setMeasuring((m) => !m)} disabled={noGl !== null} title={noGl ?? "Click two points on the board or a part; the distance is shown in mm"}>
          Measure
        </button>
        <span className="v3d-hint" role="status" aria-label="3D status">
          {status}
        </span>
      </div>
      <div className="v3d-main">
        <div className="v3d-canvas" ref={host}>
          {!webgl && <div className="v3d-msg">WebGL is not available in this window, so the 3D view cannot draw. The board properties and stack-up are still listed.</div>}
          {gpuLost && (
            <div className="v3d-msg" role="status">
              The graphics context was lost (GPU reset or out of graphics memory). The schematic and PCB editors are unaffected; the 3D view
              redraws when the context returns. If it keeps happening, choose Low resource in Settings → Performance.
            </div>
          )}
          {webgl && !projectId && <div className="v3d-msg">Open a project to see its board in 3D.</div>}
          {error && <div className="v3d-msg">{error}</div>}
        </div>
        <aside className="v3d-side" aria-label="3D properties">
          {look && (
            <section aria-label="Board appearance">
              <strong>Board</strong>
              <label>
                Solder mask
                <select aria-label="Solder mask colour" value={look.maskName} disabled={mfgWhy !== null} title={mfgWhy ?? "Stored in the project (SetManufacturing); the fabricator gets it too"} onChange={(e) => setMfg({ solder_mask_color: e.target.value })}>
                  {[...new Set([look.maskName, ...Object.keys(MASK_COLOURS)])].map((k) => (
                    <option key={k} value={k}>
                      {k}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                Finish
                <select aria-label="Surface finish" value={look.finishName} disabled={mfgWhy !== null} title={mfgWhy ?? "Stored in the project (SetManufacturing); the fabricator gets it too"} onChange={(e) => setMfg({ surface_finish: e.target.value })}>
                  {[...new Set([look.finishName, ...Object.keys(FINISHES)])].map((k) => (
                    <option key={k} value={k}>
                      {k}
                    </option>
                  ))}
                </select>
              </label>
              {look.unknown.length > 0 && <div className="muted">No 3D colour for {look.unknown.join(" and ")}: drawn with the default.</div>}
            </section>
          )}
          {stack && (
            <section aria-label="Stack-up">
              <strong>Stack-up</strong>
              <div className="muted" data-source={stack.fromIr ? "ir" : "assumed"}>
                {stack.fromIr ? `From the project stack-up: board ${stack.boardThicknessMm.toFixed(3)} mm.` : "No stack-up is set in the project: the view assumes 2 copper layers and 1.6 mm."}
              </div>
              <table className="v3d-stack" aria-label="Stack-up layers">
                <thead>
                  <tr>
                    <th>Layer</th>
                    <th>Type</th>
                    <th>µm</th>
                    <th>Value</th>
                  </tr>
                </thead>
                <tbody>
                  {stack.rows.map((r, i) => (
                    <tr key={i} data-kind={r.kind}>
                      <td>{r.name}</td>
                      <td>{r.detail}</td>
                      <td>{Number(r.thicknessUm.toFixed(1))}</td>
                      <td>{r.value}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </section>
          )}
          <section aria-label="3D layers">
            <strong>Visibility</strong>
            {toggles.map((g) => (
              <label key={g.key}>
                <input type="checkbox" checked={!hidden.has(g.key)} onChange={() => toggle(g.key)} />
                {g.colour && <i className="v3d-swatch" style={{ background: g.colour }} />}
                {g.label}
              </label>
            ))}
            <label>
              <input type="checkbox" checked={labelsOn} onChange={() => setLabelsOn((l) => !l)} />
              Placeholder labels
            </label>
          </section>
          {scene && (
            <div className="v3d-note">
              <div>
                Board {scene.thickness.toFixed(2)} mm, {scene.layers.length} copper layer{scene.layers.length === 1 ? "" : "s"}. Copper colours follow the PCB layer manager.
              </div>
              {scene.outline.length < 3 && <div>No board outline set: the board slab is not drawn.</div>}
              <div>Parts without a loaded model are placeholders, drawn with amber edges and a label: boxes from the footprint courtyard and library body height. Courtyard outlines stand in for silkscreen; zone fills are not drawn in 3D (outline only; the PCB editor draws the engine fill).</div>
              {scene.missing.length > 0 && <div>No body height in the library, outline only: {scene.missing.join(", ")}.</div>}
            </div>
          )}
          {scene && scene.parts.length > 0 && (
            <section className="v3d-models" aria-label="3D model quality">
              <strong>Models</strong>
              <ul>
                {scene.parts.map((p) => {
                  const st = modelStatus(p.model, loads[p.footprintId]?.ok === true);
                  const why = placeholderReason(p.model != null, loads[p.footprintId], scene.bodies.some((b) => b.footprintId === p.footprintId));
                  return (
                    <li key={p.footprintId} className={p.footprintId === selectedFp ? "on" : ""} data-status={st}>
                      <button type="button" aria-pressed={p.footprintId === selectedFp} onClick={() => pick.current(p.footprintId)} title={p.model ? modelKey(p.model) : "Select this part (also in the schematic and PCB)"}>
                        {p.refdes}
                      </button>{" "}
                      <span data-status={st}>{STATUS_TEXT[st]}</span>
                      {why && <span className="muted"> — {why}</span>}
                    </li>
                  );
                })}
              </ul>
              <button type="button" disabled={modelCount === 0} title={modelCount === 0 ? "No part references a 3D model file" : "Re-read every attached model file (after you replaced one on disk)"} onClick={reloadModels}>
                Reload models
              </button>
            </section>
          )}
          {selPart && snapshot && (
            <ModelAssign
              key={selPart.footprintId}
              refdes={selPart.refdes}
              def={snapshot.project.library?.[selPart.componentId] as unknown as ComponentDef | undefined}
              execute={execute}
              busy={busy}
              onProbe={onProbe}
            />
          )}
        </aside>
      </div>
    </div>
  );
}

/** Attach, replace or remove the selected part's 3D model (one `UpsertComponentDefinition`). */
function ModelAssign({ refdes, def, execute, busy, onProbe }: { refdes: string; def: ComponentDef | undefined; execute?: (c: Command) => void | Promise<void>; busy: boolean; onProbe?: (kind: "pcb" | "schematic") => void }) {
  const current = (def as unknown as { model3d?: { model_ref?: ModelRef | null } } | undefined)?.model3d?.model_ref ?? null;
  const [f, setF] = useState<ModelForm>(() => modelFormOf(current));
  const check = modelRefFromForm(f);
  const why = !execute ? "Unavailable: this view has no engine connection" : !def ? "Unavailable: the part's definition is not in the project library" : busy ? "An engine command is running" : null;
  const withModel = (ref: ModelRef | null): Command | null =>
    def ? { type: "UpsertComponentDefinition", definition: { ...def, model3d: { kind: "box", color: "#808080", external_ref: null, ...def.model3d, model_ref: ref } } } : null;
  const run = (c: Command | null) => {
    if (c) void execute?.(c);
  };
  const browse = async () => {
    const p = await pickModelFile("Choose a 3D model (STEP, WRL or GLB)");
    if (p) setF((x) => ({ ...x, path: p }));
  };
  return (
    <section className="v3d-assign" aria-label="Selected part">
      <strong>{refdes}</strong>
      {onProbe && (
        <div className="row">
          <button type="button" onClick={() => onProbe("pcb")} title="Open the PCB with this part selected">
            Show in PCB
          </button>
          <button type="button" onClick={() => onProbe("schematic")} title="Open the schematic sheet with this part selected">
            Show in schematic
          </button>
        </div>
      )}
      <label>
        Model file
        <input aria-label="Selected part 3D model path" value={f.path} disabled={why !== null} title={why ?? undefined} onChange={(e) => setF({ ...f, path: e.target.value })} placeholder=".step, .stp, .wrl or .glb" />
      </label>
      <button type="button" disabled={why !== null} title={why ?? "Choose a model file"} onClick={() => void browse()}>
        Browse…
      </button>
      <label>
        Quality
        <select aria-label="Selected part 3D model quality" value={f.quality} disabled={why !== null} title={why ?? undefined} onChange={(e) => setF({ ...f, quality: e.target.value as ModelQuality | "" })}>
          {QUALITIES.map((q) => (
            <option key={q.value} value={q.value}>
              {q.label}
            </option>
          ))}
        </select>
      </label>
      <label>
        Provenance
        <input aria-label="Selected part 3D model provenance" value={f.provenance} disabled={why !== null} title={why ?? undefined} onChange={(e) => setF({ ...f, provenance: e.target.value })} placeholder="Library, URL or author" />
      </label>
      {"problem" in check && <div className="muted">{check.problem}</div>}
      <div className="row">
        <button type="button" disabled={why !== null || !("ref" in check)} title={why ?? ("problem" in check ? check.problem : "Stored on the part definition (UpsertComponentDefinition)")} onClick={() => run("ref" in check ? withModel(check.ref) : null)}>
          {current ? "Update model" : "Assign model"}
        </button>
        {current && (
          <button type="button" disabled={why !== null} title={why ?? "Remove the model from the part definition"} onClick={() => run(withModel(null))}>
            Remove model
          </button>
        )}
      </div>
    </section>
  );
}
