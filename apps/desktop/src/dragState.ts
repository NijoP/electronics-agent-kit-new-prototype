/**
 * In-memory drag-and-drop state for component dragging from the Library/Parts panel
 * to the schematic canvas.
 */
export const EAK_COMPONENT_MIME = "application/x-eak-component";

export interface DraggedComponent {
  id: string;
  name: string;
  category?: string;
  footprint?: string;
}

let activeDraggedComponent: DraggedComponent | null = null;

export function setDraggedComponent(c: DraggedComponent | null) {
  activeDraggedComponent = c;
}

export function getDraggedComponent(): DraggedComponent | null {
  return activeDraggedComponent;
}
