import { MouseEvent } from "react";
import { useGraphStore } from "../store/graphStore";

export function useGraphInteractions() {
  const menu = useGraphStore((state) => state.menu);
  const setMenu = useGraphStore((state) => state.setMenu);
  const clearMenu = useGraphStore((state) => state.clearMenu);

  const onCanvasContextMenu = (event: MouseEvent) => {
    event.preventDefault();
    setMenu({ x: event.clientX, y: event.clientY });
  };

  return {
    menu,
    onCanvasContextMenu,
    onPaneClick: clearMenu,
  };
}
