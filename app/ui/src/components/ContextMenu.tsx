import { useReactFlow } from "@xyflow/react";
import { AppWindow, Mic, MonitorSpeaker, Volume2 } from "lucide-react";
import { type AudioFlowNode, type AudioNodeKind, useGraphStore } from "../store/graphStore";

type Props = {
  x: number;
  y: number;
};

const entries: Array<{ kind: AudioNodeKind; label: string; icon: typeof Mic }> = [
  { kind: "inputDevice", label: "Entrée", icon: Mic },
  { kind: "outputDevice", label: "Sortie", icon: Volume2 },
  { kind: "application", label: "Application", icon: AppWindow },
  { kind: "virtualOutput", label: "VAM Sortie", icon: MonitorSpeaker },
];

export function ContextMenu({ x, y }: Props) {
  const { screenToFlowPosition } = useReactFlow<AudioFlowNode>();
  const addNode = useGraphStore((state) => state.addNode);
  const clearMenu = useGraphStore((state) => state.clearMenu);

  return (
    <div className="context-menu" style={{ left: x, top: y }}>
      {entries.map((entry) => {
        const Icon = entry.icon;
        return (
          <button
            key={entry.kind}
            onClick={() => {
              addNode(
                entry.kind,
                entry.kind === "virtualOutput" ? "VAM Sortie (Bubux Audio Driver)" : undefined,
                screenToFlowPosition({ x, y }),
                entry.kind === "application" ? { channels: 2, channelNames: ["L", "R"] } : undefined,
              );
              clearMenu();
            }}
          >
            <Icon size={15} />
            {entry.label}
          </button>
        );
      })}
    </div>
  );
}
