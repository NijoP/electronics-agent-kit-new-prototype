// The open 3D document's controls, published for the action registry (A1): the menus, palette and
// keys of the 3D document drive the same functions as the viewer's own toolbar. Only one 3D
// document exists per window; while none is mounted the 3D actions say so instead of acting.
import { useSyncExternalStore } from "react";
import type { Projection, View3dPreset } from "./view";

export interface View3dControls {
  projection: Projection;
  measuring: boolean;
  /** A completed or started measurement is on screen. */
  measured: boolean;
  /** Placeholder labels (parts without a loaded model) are shown. */
  labels: boolean;
  /** Number of parts that reference a model file (what Reload re-reads). */
  modelCount: number;
  preset: (p: View3dPreset) => void;
  reset: () => void;
  zoomBy: (factor: number) => void;
  setProjection: (p: Projection) => void;
  toggleMeasure: () => void;
  clearMeasure: () => void;
  toggleLabels: () => void;
  reloadModels: () => void;
}

let current: View3dControls | null = null;
let version = 0;
const listeners = new Set<() => void>();

/** Called by the mounted viewer whenever its state changes; `null` when it unmounts. */
export function publishView3d(c: View3dControls | null) {
  current = c;
  version += 1;
  listeners.forEach((l) => l());
}

export const view3dControls = (): View3dControls | null => current;

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
};

/** The published controls, re-rendering the caller when they change (checked menu items). */
export function useView3dControls(): View3dControls | null {
  useSyncExternalStore(subscribe, () => version);
  return current;
}
