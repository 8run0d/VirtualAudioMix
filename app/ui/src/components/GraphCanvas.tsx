import { useEffect, useRef, useState } from "react";
import {
  Background,
  Connection,
  Controls,
  EdgeMouseHandler,
  MiniMap,
  NodeMouseHandler,
  ReactFlow,
  type AriaLabelConfig,
  useReactFlow,
} from "@xyflow/react";
import { Eye, EyeOff } from "lucide-react";
import { ContextMenu } from "./ContextMenu";
import { Edge } from "./Edge";
import { EdgeContextMenu } from "./EdgeContextMenu";
import { Node } from "./Node";
import { NodeContextMenu } from "./NodeContextMenu";
import { useGraphInteractions } from "../hooks/useGraphInteractions";
import { useAudioStore } from "../store/audioStore";
import { AudioFlowNode, useGraphStore } from "../store/graphStore";

const nodeTypes = {
  audioNode: Node,
};

const edgeTypes = {
  audioEdge: Edge,
};

const frenchReactFlowLabels: Partial<AriaLabelConfig> = {
  "controls.ariaLabel": "Contrôles du canevas",
  "controls.zoomIn.ariaLabel": "Zoomer",
  "controls.zoomOut.ariaLabel": "Dézoomer",
  "controls.fitView.ariaLabel": "Ajuster la vue",
  "controls.interactive.ariaLabel": "Verrouiller ou déverrouiller les interactions",
  "minimap.ariaLabel": "Mini-carte du graphe audio",
  "handle.ariaLabel": "Point de connexion",
};

export function GraphCanvas() {
  const [isMiniMapVisible, setIsMiniMapVisible] = useState(true);
  const [nodeMenu, setNodeMenu] = useState<{ nodeId: string; x: number; y: number } | null>(null);
  const [edgeMenu, setEdgeMenu] = useState<{ edgeId: string; x: number; y: number } | null>(null);
  const initialFitDoneRef = useRef(false);
  const { fitView } = useReactFlow<AudioFlowNode>();
  const nodes = useGraphStore((state) => state.nodes);
  const edges = useGraphStore((state) => state.edges);
  const setNodes = useGraphStore((state) => state.setNodes);
  const setEdges = useGraphStore((state) => state.setEdges);
  const addConnection = useGraphStore((state) => state.addConnection);
  const removeSelectedEdges = useGraphStore((state) => state.removeSelectedEdges);
  const updateUiLevels = useGraphStore((state) => state.updateUiLevels);
  const getAudioNodeLevels = useAudioStore((state) => state.getAudioNodeLevels);
  const { menu, onCanvasContextMenu, onPaneClick } = useGraphInteractions();

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target;
      if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement) {
        return;
      }

      if (event.key === "Delete" || event.key === "Backspace") {
        removeSelectedEdges();
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [removeSelectedEdges]);

  useEffect(() => {
    let disposed = false;
    const refreshLevels = async () => {
      const levels = await getAudioNodeLevels();
      if (disposed) {
        return;
      }

      updateUiLevels(Object.fromEntries(levels.map((level) => [level.nodeId, level])));
    };
    const timer = window.setInterval(() => {
      void refreshLevels();
    }, 90);
    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, [getAudioNodeLevels, updateUiLevels]);

  useEffect(() => {
    if (initialFitDoneRef.current || nodes.length === 0) {
      return;
    }

    const timer = window.setTimeout(() => {
      initialFitDoneRef.current = true;
      void fitView({ padding: 0.18, duration: 220, includeHiddenNodes: false });
    }, 800);

    return () => window.clearTimeout(timer);
  }, [edges.length, fitView, nodes.length]);

  const onConnect = (connection: Connection) => {
    addConnection(connection);
  };

  const onNodeContextMenu: NodeMouseHandler<AudioFlowNode> = (event, node) => {
    event.preventDefault();
    event.stopPropagation();
    setNodeMenu({ nodeId: node.id, x: event.clientX, y: event.clientY });
    setEdgeMenu(null);
  };

  const onEdgeContextMenu: EdgeMouseHandler = (event, edge) => {
    event.preventDefault();
    event.stopPropagation();
    setEdgeMenu({ edgeId: edge.id, x: event.clientX, y: event.clientY });
    setNodeMenu(null);
  };

  const closeMenus = () => {
    onPaneClick();
    setNodeMenu(null);
    setEdgeMenu(null);
  };

  return (
    <section className="canvas" onContextMenu={onCanvasContextMenu}>
      <ReactFlow
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        edgeTypes={edgeTypes}
        onNodesChange={setNodes}
        onEdgesChange={setEdges}
        onConnect={onConnect}
        onNodeContextMenu={onNodeContextMenu}
        onEdgeContextMenu={onEdgeContextMenu}
        onPaneClick={closeMenus}
        ariaLabelConfig={frenchReactFlowLabels}
        fitView
      >
        <Background color="#151515" gap={24} />
        <Controls position="bottom-left" />
        {isMiniMapVisible ? (
          <MiniMap
            pannable
            zoomable
            position="bottom-right"
            bgColor="#050505"
            maskColor="rgba(0, 0, 0, 0.72)"
            nodeColor="#1a1a1a"
            nodeStrokeColor="#3a3a3a"
            nodeBorderRadius={4}
          />
        ) : null}
      </ReactFlow>
      <button
        className="minimap-toggle"
        onClick={() => setIsMiniMapVisible((visible) => !visible)}
        title={isMiniMapVisible ? "Masquer la minimap" : "Afficher la minimap"}
      >
        {isMiniMapVisible ? <EyeOff size={15} /> : <Eye size={15} />}
      </button>
      {menu ? <ContextMenu x={menu.x} y={menu.y} /> : null}
      {nodeMenu ? (
        <NodeContextMenu
          nodeId={nodeMenu.nodeId}
          x={nodeMenu.x}
          y={nodeMenu.y}
          onClose={() => setNodeMenu(null)}
        />
      ) : null}
      {edgeMenu ? (
        <EdgeContextMenu
          edgeId={edgeMenu.edgeId}
          x={edgeMenu.x}
          y={edgeMenu.y}
          onClose={() => setEdgeMenu(null)}
        />
      ) : null}
    </section>
  );
}
