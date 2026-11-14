// occt-import-js (LGPL-2.1, unmodified npm dependency, loaded lazily) ships no type declarations.
// Only the subset used by src/viewer3d/modelLoader.ts is declared.
declare module "occt-import-js" {
  export interface OcctMesh {
    name?: string;
    color?: [number, number, number];
    attributes: { position: { array: ArrayLike<number> }; normal?: { array: ArrayLike<number> } };
    index: { array: ArrayLike<number> };
  }
  export interface OcctResult {
    success: boolean;
    meshes: OcctMesh[];
  }
  export interface Occt {
    ReadStepFile(content: Uint8Array, params: unknown): OcctResult;
  }
  export default function occtimportjs(options?: { locateFile?: (name: string) => string }): Promise<Occt>;
}
