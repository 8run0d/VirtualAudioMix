import { Trash2 } from "lucide-react";
import { useGraphStore } from "../store/graphStore";

type Props = {
  edgeId: string;
  x: number;
  y: number;
  onClose: () => void;
};

export function EdgeContextMenu({ edgeId, x, y, onClose }: Props) {
  const removeEdge = useGraphStore((state) => state.removeEdge);

  return (
    <div className="context-menu" style={{ left: x, top: y }}>
      <button
        className="destructive-menu-item"
        onClick={() => {
          removeEdge(edgeId);
          onClose();
        }}
      >
        <Trash2 size={15} />
        Supprimer le lien
      </button>
    </div>
  );
}
