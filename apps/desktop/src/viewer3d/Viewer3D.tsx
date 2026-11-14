// 3D board viewer (three.js): board outline extruded at the stackup thickness, copper layers,
// pads, tracks, vias, courtyard outlines and component bodies as plain boxes. Orbit / pan / zoom,
// top / bottom / iso presets and per-layer visibility. It re-reads the engine snapshot whenever the
// project revision changes, so edits made anywhere (UI, CLI, AI) show up here. Read-only: it never
// issues a command, and geometry is derived data, not electrical truth.
import { useEffect, useMemo, useRef, useState } from "react";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { api } from "../api";
import type { Snapshot } from "../types";
import { buildScene, type BoardScene } from "./scene";

interface Props {
  projectId: string | null;
  revision: number | null;
  active: boolean;
}

type Preset = "top" | "bottom" | "iso";

/** Layer-group keys beyond the copper layers. */
const GROUPS = [
  { key: "board", label: "Board" },
  { key: "bodies-top", label: "Top bodies" },
  { key: "bodies-bottom", label: "Bottom bodies" },
  { key: "outline-top", label: "Top courtyards" },
  { key: "outline-bottom", label: "Bottom courtyards" },
  { key: "vias", label: "Vias" },
] as const;

const MAT = {
  board: { color: 0x1f5d3a, opacity: 0.72 },
  body: 0x3a3f47,
};

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

/** Build the three.js groups, keyed by visibility group. PCB (x, y) maps to world (X, Z); Y is up. */
export function buildGroups(sc: BoardScene): Map<string, THREE.Group> {
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
    const mesh = new THREE.Mesh(geo, new THREE.MeshStandardMaterial({ color: MAT.board.color, transparent: true, opacity: MAT.board.opacity, roughness: 0.8, side: THREE.DoubleSide }));
    mesh.name = "board-slab";
    group("board").add(mesh);
  }

  const layerOf = new Map(sc.layers.map((l) => [l.key, l]));
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
    for (const m of [instanced(box, l.colour, tracks), instanced(box, 0xd8b45a, boxPads), instanced(cyl, 0xd8b45a, roundPads)]) if (m) g.add(m);
    for (const z of sc.zones.filter((zz) => zz.layer === l.key)) {
      const pts = z.outline.map(([x, y]) => new THREE.Vector3(x, l.y + l.thickness, y));
      if (pts.length > 1) g.add(new THREE.LineLoop(new THREE.BufferGeometry().setFromPoints(pts), new THREE.LineBasicMaterial({ color: l.colour })));
    }
  }

  const vias = sc.vias.map((v) => [v.x, (v.yTop + v.yBottom) / 2, v.y, v.diameter, v.yTop - v.yBottom, v.diameter, 0]);
  const viaMesh = instanced(cyl, 0xd8b45a, vias);
  if (viaMesh) group("vias").add(viaMesh);

  const topCu = layerOf.get(sc.layers[0].key)!;
  const botCu = layerOf.get(sc.layers[sc.layers.length - 1].key)!;
  for (const side of ["top", "bottom"] as const) {
    const edge = side === "top" ? topCu.y + topCu.thickness / 2 : botCu.y - botCu.thickness / 2;
    const dir = side === "top" ? 1 : -1;
    const bodies = sc.bodies.filter((b) => b.side === side).map((b) => [b.x, edge + (dir * b.height) / 2, b.y, b.sx, b.height, b.sy, 0]);
    const m = instanced(box, MAT.body, bodies);
    if (m) group(`bodies-${side}`).add(m);
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

export function Viewer3D({ projectId, revision, active }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const three = useRef<{
    camera: THREE.PerspectiveCamera;
    controls: OrbitControls;
    render: () => void;
    content: THREE.Group;
    groups: Map<string, THREE.Group>;
  } | null>(null);
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [webgl, setWebgl] = useState(true);
  const [ready, setReady] = useState(false);
  const [hidden, setHidden] = useState<Set<string>>(new Set());
  const hiddenRef = useRef(hidden);
  hiddenRef.current = hidden;
  const fitted = useRef<string | null>(null);

  const scene = useMemo(() => (snapshot ? buildScene(snapshot) : null), [snapshot]);

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
        if (!cancelled) setError(typeof e === "string" ? e : (e as { message?: string })?.message ?? "Could not read the project");
      });
    return () => {
      cancelled = true;
    };
  }, [active, projectId, revision]);

  // Renderer, camera and controls: created once per mount.
  useEffect(() => {
    const el = host.current;
    if (!el) return;
    let renderer: THREE.WebGLRenderer;
    try {
      renderer = new THREE.WebGLRenderer({ antialias: true });
    } catch {
      setWebgl(false);
      return;
    }
    renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
    renderer.setClearColor(0x14181d);
    el.appendChild(renderer.domElement);
    renderer.domElement.style.display = "block";
    const sc = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(40, 1, 0.1, 5000);
    camera.position.set(0, 80, 60);
    sc.add(new THREE.HemisphereLight(0xffffff, 0x303844, 1.1));
    const sun = new THREE.DirectionalLight(0xffffff, 1.4);
    sun.position.set(40, 120, 60);
    sc.add(sun);
    const fill = new THREE.DirectionalLight(0xffffff, 0.6);
    fill.position.set(-40, -120, -60);
    sc.add(fill);
    const content = new THREE.Group();
    sc.add(content);
    const controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = false;
    controls.screenSpacePanning = true;
    controls.maxPolarAngle = Math.PI;

    let queued = false;
    const render = () => {
      if (queued) return;
      queued = true;
      requestAnimationFrame(() => {
        queued = false;
        renderer.render(sc, camera);
      });
    };
    controls.addEventListener("change", render);
    const resize = () => {
      const w = Math.max(el.clientWidth, 1);
      const h = Math.max(el.clientHeight, 1);
      renderer.setSize(w, h, false);
      renderer.domElement.style.width = `${w}px`;
      renderer.domElement.style.height = `${h}px`;
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
      render();
    };
    const ro = new ResizeObserver(resize);
    ro.observe(el);
    resize();
    three.current = { camera, controls, render, content, groups: new Map() };
    setReady(true);
    return () => {
      ro.disconnect();
      controls.dispose();
      disposeGroup(content);
      renderer.dispose();
      renderer.domElement.remove();
      three.current = null;
      setReady(false);
    };
  }, []);

  const preset = (p: Preset, sc: BoardScene | null = scene) => {
    const t = three.current;
    if (!t || !sc) return;
    const [x0, y0, x1, y1] = sc.bounds;
    const cx = (x0 + x1) / 2;
    const cz = (y0 + y1) / 2;
    const r = Math.max(Math.hypot(x1 - x0, y1 - y0) / 2, 5);
    const d = (r / Math.tan((t.camera.fov * Math.PI) / 360)) * 1.15;
    t.controls.target.set(cx, 0, cz);
    // A tiny Z offset keeps the up-vector well defined when looking straight down / up.
    if (p === "top") t.camera.position.set(cx, d, cz + d * 0.001);
    else if (p === "bottom") t.camera.position.set(cx, -d, cz + d * 0.001);
    else t.camera.position.set(cx + d * 0.55, d * 0.7, cz + d * 0.7);
    t.camera.near = Math.max(d / 500, 0.05);
    t.camera.far = d * 20;
    t.camera.updateProjectionMatrix();
    t.controls.update();
    t.render();
  };

  // Rebuild geometry when the scene data changes; keep the user's camera except on the first load.
  useEffect(() => {
    const t = three.current;
    if (!t || !scene) return;
    t.content.clear();
    t.groups.forEach(disposeGroup);
    const groups = buildGroups(scene);
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
  }, [scene, ready]);

  // Layer visibility.
  useEffect(() => {
    const t = three.current;
    if (!t) return;
    t.groups.forEach((g, k) => {
      g.visible = !hidden.has(k);
    });
    t.render();
  }, [hidden]);

  // Becoming visible again after being hidden: the canvas was 0-sized, so redraw.
  useEffect(() => {
    if (active) three.current?.render();
  }, [active]);

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

  return (
    <div className="viewer3d" aria-label="3D board viewer">
      <div className="v3d-bar" role="toolbar" aria-label="3D view">
        <button type="button" onClick={() => preset("top")} title="Look straight down at the top side">Top</button>
        <button type="button" onClick={() => preset("bottom")} title="Look up at the bottom side">Bottom</button>
        <button type="button" onClick={() => preset("iso")} title="Isometric view, fit to board">Iso / fit</button>
        <span className="v3d-hint">Left-drag orbit · right-drag pan · wheel zoom</span>
      </div>
      <div className="v3d-main">
        <div className="v3d-canvas" ref={host}>
          {!webgl && <div className="v3d-msg">WebGL is not available in this window, so the 3D view cannot draw.</div>}
          {webgl && !projectId && <div className="v3d-msg">Open a project to see its board in 3D.</div>}
          {error && <div className="v3d-msg">{error}</div>}
        </div>
        <aside className="v3d-side" aria-label="3D layers">
          <strong>Layers</strong>
          {toggles.map((g) => (
            <label key={g.key}>
              <input type="checkbox" checked={!hidden.has(g.key)} onChange={() => toggle(g.key)} />
              {g.colour && <i className="v3d-swatch" style={{ background: g.colour }} />}
              {g.label}
            </label>
          ))}
          {scene && (
            <div className="v3d-note">
              <div>
                Board {scene.thickness.toFixed(2)} mm, {scene.layers.length} copper layer{scene.layers.length === 1 ? "" : "s"}.
              </div>
              {scene.outline.length < 3 && <div>No board outline set: the board slab is not drawn.</div>}
              <div>Bodies are boxes from footprint courtyard and library body height; courtyard outlines stand in for silkscreen; zone fills are not computed (outline only).</div>
              {scene.missing.length > 0 && <div>No body height in the library, not drawn: {scene.missing.join(", ")}.</div>}
            </div>
          )}
        </aside>
      </div>
    </div>
  );
}
