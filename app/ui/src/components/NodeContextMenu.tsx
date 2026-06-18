import { Trash2 } from "lucide-react";
import { useGraphStore } from "../store/graphStore";

type Props = {
  nodeId: string;
  x: number;
  y: number;
  onClose: () => void;
};

export function NodeContextMenu({ nodeId, x, y, onClose }: Props) {
  const removeNode = useGraphStore((state) => state.removeNode);

  return (
    <div className="context-menu" style={{ left: x, top: y }}>
      <button
        className="destructive-menu-item"
        onClick={() => {
          removeNode(nodeId);
          onClose();
        }}
      >
        <Trash2 size={15} />
        Supprimer
      </button>
    </div>
  );
}
