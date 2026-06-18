import { Connection, Edge, EdgeChange, Node, NodeChange, XYPosition, addEdge, applyEdgeChanges, applyNodeChanges } from "@xyflow/react";
import { create } from "zustand";

export type AudioNodeKind = "inputDevice" | "outputDevice" | "application" | "systemAudio" | "virtualOutput";

export type AudioNodeData = Record<string, unknown> & {
  kind: AudioNodeKind;
  label: string;
  channels?: number;
  channelNames?: string[];
  expanded?: boolean;
  level: number;
  bands?: number[];
  waveform?: number[];
  visualGain?: number;
  enabled?: boolean;
  deviceMissing?: boolean;
  processId?: number | null;
};

export type AudioEdgeData = Record<string, unknown> & {
  gain: number;
  level: number;
  sourceChannel?: number | "all";
  targetChannel?: number | "all";
};

export type AudioFlowNode = Node<AudioNodeData, "audioNode">;
export type AudioFlowEdge = Edge<AudioEdgeData, "audioEdge">;

type MenuState = {
  x: number;
  y: number;
};

type UiNodeLevel = {
  level: number;
  bands?: number[];
  waveform?: number[];
};

type AddNodeOptions = {
  channels?: number;
  channelNames?: string[];
  expanded?: boolean;
  processId?: number | null;
};

type GraphState = {
  nodes: AudioFlowNode[];
  edges: AudioFlowEdge[];
  menu: MenuState | null;
  addNode: (kind: AudioNodeKind, label?: string, position?: XYPosition, options?: AddNodeOptions) => void;
  addConnection: (connection: Connection) => void;
  toggleNodeExpanded: (nodeId: string) => void;
  setEdgeGain: (edgeId: string, gain: number) => void;
  toggleNodeEnabled: (nodeId: string) => void;
  updateUiLevels: (levels?: Record<string, UiNodeLevel>) => void;
  removeEdge: (edgeId: string) => void;
  removeSelectedEdges: () => void;
  removeNode: (nodeId: string) => void;
  applyDefaultRouting: (inputDeviceName: string, outputDeviceName: string) => void;
  replaceGraph: (nodes: AudioFlowNode[], edges: AudioFlowEdge[]) => void;
  markDeviceAvailability: (availableDeviceNames: string[]) => void;
  setNodes: (changes: NodeChange<AudioFlowNode>[]) => void;
  setEdges: (changes: EdgeChange<AudioFlowEdge>[]) => void;
  setMenu: (menu: MenuState) => void;
  clearMenu: () => void;
};

const labels: Record<AudioNodeKind, string> = {
  inputDevice: "Entrée",
  outputDevice: "Sortie",
  application: "Application",
  systemAudio: "Son système",
  virtualOutput: "VAM Sortie",
};

let nodeSequence = 0;
let edgeSequence = 0;

function createAudioNode(kind: AudioNodeKind, label: string, position: XYPosition, options: AddNodeOptions = {}): AudioFlowNode {
  const channels = sanitizeChannelCount(options.channels);
  return {
    id: `node-${nodeSequence++}`,
    type: "audioNode",
    position,
    data: {
      kind,
      label,
      channels,
      channelNames: normalizeChannelNames(channels, options.channelNames, kind),
      expanded: options.expanded ?? false,
      level: 0,
      bands: [0, 0, 0, 0, 0, 0, 0, 0],
      waveform: zeroWaveform(),
      visualGain: 1,
      enabled: true,
      deviceMissing: false,
      processId: options.processId ?? null,
    },
  };
}

function createAudioEdge(source: string, target: string): AudioFlowEdge {
  return {
    id: `edge-${edgeSequence++}`,
    source,
    target,
    sourceHandle: "all",
    targetHandle: "all",
    type: "audioEdge",
    data: { gain: 1, level: 0, sourceChannel: "all", targetChannel: "all" },
  };
}

function createInitialNodes() {
  return [
    createAudioNode("virtualOutput", "VAM Sortie (Bubux Audio Driver)", { x: 180, y: 170 }, { channels: 2 }),
    createAudioNode("inputDevice", "VAM Entrée (Bubux Audio Driver)", { x: 560, y: 170 }, { channels: 2 }),
  ];
}

export const useGraphStore = create<GraphState>((set, get) => ({
  nodes: createInitialNodes(),
  edges: [],
  menu: null,
  addNode: (kind, label, position, options = {}) => {
    const existingNodes = get().nodes;
    const nodeId = nextNodeId(existingNodes);
    const index = existingNodes.length;
    const channels = sanitizeChannelCount(options.channels);
    const node: AudioFlowNode = {
      id: nodeId,
      type: "audioNode",
      position: position ?? { x: 180 + (index % 4) * 220, y: 120 + Math.floor(index / 4) * 140 },
      data: {
        kind,
        label: label ?? labels[kind],
        channels,
        channelNames: normalizeChannelNames(channels, options.channelNames, kind),
        expanded: options.expanded ?? false,
        level: 0,
        bands: [0, 0, 0, 0, 0, 0, 0, 0],
        waveform: zeroWaveform(),
        visualGain: 1,
        enabled: true,
        deviceMissing: false,
        processId: options.processId ?? null,
      },
    };
    set({ nodes: [...existingNodes, node] });
  },
  addConnection: (connection) => {
    const sourceChannel = parseChannelHandle(connection.sourceHandle);
    const targetChannel = parseChannelHandle(connection.targetHandle);
    const nextEdges = addEdge<AudioFlowEdge>(
      {
        ...connection,
        type: "audioEdge",
        data: {
          gain: 1,
          level: 0,
          sourceChannel,
          targetChannel,
        },
      },
      get().edges,
    );
    set({
      edges: nextEdges,
      nodes: keepChannelLinkedNodesExpanded(get().nodes, nextEdges),
    });
  },
  toggleNodeExpanded: (nodeId) => {
    const mustStayExpanded = get().edges.some(
      (edge) =>
        (edge.source === nodeId && edge.data?.sourceChannel !== undefined && edge.data.sourceChannel !== "all") ||
        (edge.target === nodeId && edge.data?.targetChannel !== undefined && edge.data.targetChannel !== "all"),
    );
    set({
      nodes: get().nodes.map((node) =>
        node.id === nodeId
          ? {
              ...node,
              data: {
                ...node.data,
                expanded: mustStayExpanded ? true : !node.data.expanded,
              },
            }
          : node,
      ),
    });
  },
  setEdgeGain: (edgeId, gain) => {
    const safeGain = Number.isFinite(gain) ? Math.min(4, Math.max(0, gain)) : 1;
    set({
      edges: get().edges.map((edge) =>
        edge.id === edgeId
          ? {
              ...edge,
              data: {
                ...edge.data,
                gain: safeGain,
                level: edge.data?.level ?? 0,
              },
            }
          : edge,
      ),
    });
  },
  toggleNodeEnabled: (nodeId) => {
    const nodes = get().nodes.map((node) =>
      node.id === nodeId
        ? {
            ...node,
            data: {
              ...node.data,
              enabled: node.data.enabled === false,
              level: node.data.enabled === false ? node.data.level : 0,
              bands: node.data.enabled === false ? node.data.bands : [0, 0, 0, 0, 0, 0, 0, 0],
              waveform: node.data.enabled === false ? node.data.waveform : zeroWaveform(),
            },
          }
        : node,
    );
    const activeNodeIds = new Set(nodes.filter((node) => node.data.enabled !== false).map((node) => node.id));
    set({
      nodes,
      edges: get().edges.map((edge) => ({
        ...edge,
        data: {
          gain: edge.data?.gain ?? 1,
          level: activeNodeIds.has(edge.source) && activeNodeIds.has(edge.target) ? (edge.data?.level ?? 0) : 0,
        },
      })),
    });
  },
  updateUiLevels: (levels = {}) => {
    const activeNodeIds = new Set(get().nodes.filter((node) => node.data.enabled !== false).map((node) => node.id));
    const edges = get().edges;
    const nextNodes = get().nodes.map((node) => {
      const enabled = node.data.enabled !== false;
      const measured = levels[node.id] ?? { level: 0, bands: [0, 0, 0, 0, 0, 0, 0, 0], waveform: zeroWaveform() };
      const nextLevel = enabled ? smoothLevel(node.data.level ?? 0, measured.level) : 0;
      const nextBands = enabled ? smoothBands(node.data.bands, measured.bands) : [0, 0, 0, 0, 0, 0, 0, 0];
      const nextWaveform = enabled ? smoothWaveform(node.data.waveform, measured.waveform) : zeroWaveform();
      return {
        ...node,
        data: {
          ...node.data,
          enabled,
          level: nextLevel,
          bands: nextBands,
          waveform: nextWaveform,
          visualGain: enabled ? visualGainForNode(node.id, edges, activeNodeIds) : 0,
        },
      };
    });
    set({
      nodes: nextNodes,
      edges: edges.map((edge) => {
        const source = nextNodes.find((node) => node.id === edge.source);
        const target = nextNodes.find((node) => node.id === edge.target);
        const active = activeNodeIds.has(edge.source) && activeNodeIds.has(edge.target);
        const sourceLevel = source?.data.level ?? 0;
        const targetLevel = target?.data.level ?? 0;
        return {
          ...edge,
          data: {
            ...edge.data,
            gain: edge.data?.gain ?? 1,
            level: active ? Math.min(1, (sourceLevel + targetLevel) * 0.5 * (edge.data?.gain ?? 1)) : 0,
          },
        };
      }),
    });
  },
  removeEdge: (edgeId) => {
    const nextEdges = get().edges.filter((edge) => edge.id !== edgeId);
    set({ edges: nextEdges, nodes: keepChannelLinkedNodesExpanded(get().nodes, nextEdges) });
  },
  removeSelectedEdges: () => {
    const nextEdges = get().edges.filter((edge) => !edge.selected);
    set({ edges: nextEdges, nodes: keepChannelLinkedNodesExpanded(get().nodes, nextEdges) });
  },
  removeNode: (nodeId) => {
    set({
      nodes: get().nodes.filter((node) => node.id !== nodeId),
      edges: get().edges.filter((edge) => edge.source !== nodeId && edge.target !== nodeId),
    });
  },
  applyDefaultRouting: (inputDeviceName, outputDeviceName) => {
    const micNode = createAudioNode("inputDevice", inputDeviceName, { x: 180, y: 120 });
    const virtualInputNode = createAudioNode("inputDevice", "VAM Entrée (Bubux Audio Driver)", { x: 570, y: 120 }, { channels: 2 });
    const virtualOutputNode = createAudioNode("virtualOutput", "VAM Sortie (Bubux Audio Driver)", { x: 180, y: 320 }, { channels: 2 });
    const outputNode = createAudioNode("outputDevice", outputDeviceName, { x: 570, y: 320 }, { channels: 2 });

    set({
      nodes: [micNode, virtualInputNode, virtualOutputNode, outputNode],
      edges: [createAudioEdge(micNode.id, virtualInputNode.id), createAudioEdge(virtualOutputNode.id, outputNode.id)],
    });
  },
  replaceGraph: (nodes, edges) => {
    const normalizedNodes = nodes.map((node) => ({
        ...node,
        data: {
          ...node.data,
          channels: sanitizeChannelCount(node.data.channels),
          channelNames: normalizeChannelNames(sanitizeChannelCount(node.data.channels), node.data.channelNames, node.data.kind),
          expanded: node.data.expanded === true,
          enabled: node.data.enabled !== false,
          deviceMissing: false,
          processId: Number.isFinite(Number(node.data.processId)) ? Number(node.data.processId) : null,
          level: node.data.level ?? 0,
          bands: normalizeBands(node.data.bands),
          waveform: normalizeWaveform(node.data.waveform),
          visualGain: Number(node.data.visualGain ?? 1),
        },
      }));
    const normalizedEdges = edges.map((edge) => ({
        ...edge,
        sourceHandle: normalizeHandleId(edge.sourceHandle, edge.data?.sourceChannel),
        targetHandle: normalizeHandleId(edge.targetHandle, edge.data?.targetChannel),
        data: {
          ...edge.data,
          gain: edge.data?.gain ?? 1,
          level: edge.data?.level ?? 0,
          sourceChannel: normalizeChannelValue(edge.data?.sourceChannel),
          targetChannel: normalizeChannelValue(edge.data?.targetChannel),
        },
      }));
    syncSequences(normalizedNodes, normalizedEdges);
    set({
      nodes: normalizedNodes,
      edges: normalizedEdges,
    });
  },
  markDeviceAvailability: (availableDeviceNames) => {
    const availableNames = new Set(availableDeviceNames);
    set({
      nodes: get().nodes.map((node) => {
        const requiresDevice =
          node.data.kind === "inputDevice" ||
          node.data.kind === "outputDevice" ||
          node.data.kind === "virtualOutput" ||
          node.data.kind === "application";
        return {
          ...node,
          data: {
            ...node.data,
            deviceMissing: requiresDevice ? !availableNames.has(node.data.label) : false,
          },
        };
      }),
    });
  },
  setNodes: (changes) => set({ nodes: applyNodeChanges(changes, get().nodes) }),
  setEdges: (changes) => set({ edges: applyEdgeChanges(changes, get().edges) }),
  setMenu: (menu) => set({ menu }),
  clearMenu: () => set({ menu: null }),
}));

function smoothLevel(current: number, target: number) {
  const clampedTarget = Math.min(1, Math.max(0, target));
  const attack = clampedTarget > current ? 0.68 : 0.24;
  return Math.min(1, Math.max(0, current * (1 - attack) + clampedTarget * attack));
}

function smoothBands(current: unknown, target: unknown) {
  const currentBands = normalizeBands(current);
  const targetBands = normalizeBands(target);
  return targetBands.map((band, index) => {
    const attack = band > currentBands[index] ? 0.72 : 0.26;
    return Math.min(1, Math.max(0, currentBands[index] * (1 - attack) + band * attack));
  });
}

function smoothWaveform(current: unknown, target: unknown) {
  const currentWaveform = normalizeWaveform(current);
  const targetWaveform = normalizeWaveform(target);
  return targetWaveform.map((point, index) => {
    const attack = Math.abs(point) > Math.abs(currentWaveform[index]) ? 0.78 : 0.34;
    return Math.min(1, Math.max(-1, currentWaveform[index] * (1 - attack) + point * attack));
  });
}

function normalizeBands(value: unknown) {
  if (!Array.isArray(value)) {
    return [0, 0, 0, 0, 0, 0, 0, 0];
  }

  return [0, 1, 2, 3, 4, 5, 6, 7].map((index) => {
    const band = Number(value[index] ?? 0);
    return Number.isFinite(band) ? Math.min(1, Math.max(0, band)) : 0;
  });
}

function normalizeWaveform(value: unknown) {
  if (!Array.isArray(value)) {
    return zeroWaveform();
  }

  return Array.from({ length: 48 }, (_, index) => {
    const point = Number(value[index] ?? 0);
    return Number.isFinite(point) ? Math.min(1, Math.max(-1, point)) : 0;
  });
}

function zeroWaveform() {
  return Array.from({ length: 48 }, () => 0);
}

function nextNodeId(nodes: AudioFlowNode[]) {
  const usedIds = new Set(nodes.map((node) => node.id));
  while (usedIds.has(`node-${nodeSequence}`)) {
    nodeSequence += 1;
  }
  return `node-${nodeSequence++}`;
}

function syncSequences(nodes: AudioFlowNode[], edges: AudioFlowEdge[]) {
  nodeSequence = Math.max(nodeSequence, maxSequence(nodes.map((node) => node.id), "node-") + 1);
  edgeSequence = Math.max(edgeSequence, maxSequence(edges.map((edge) => edge.id), "edge-") + 1);
}

function maxSequence(ids: string[], prefix: string) {
  return ids.reduce((max, id) => {
    if (!id.startsWith(prefix)) {
      return max;
    }
    const value = Number(id.slice(prefix.length));
    return Number.isInteger(value) ? Math.max(max, value) : max;
  }, -1);
}

function sanitizeChannelCount(value: unknown) {
  const channels = Number(value ?? 1);
  return Number.isFinite(channels) ? Math.min(16, Math.max(1, Math.floor(channels))) : 1;
}

function normalizeChannelNames(channels: number, value: unknown, kind: unknown) {
  const names = Array.isArray(value) ? value.map((name) => String(name)) : [];
  if (names.length >= channels) {
    return names.slice(0, channels);
  }

  const fallback = defaultChannelNames(channels, kind);
  return Array.from({ length: channels }, (_, index) => names[index] || fallback[index] || `Canal ${index + 1}`);
}

function defaultChannelNames(channels: number, kind: unknown) {
  if (channels === 1) {
    return ["Mono"];
  }
  if (kind === "inputDevice") {
    return Array.from({ length: channels }, (_, index) => `Canal ${index + 1}`);
  }
  if (channels === 2) {
    return ["L", "R"];
  }
  if (channels === 6) {
    return ["L", "R", "C", "LFE", "Ls", "Rs"];
  }
  if (channels === 8) {
    return ["L", "R", "C", "LFE", "Ls", "Rs", "Lb", "Rb"];
  }
  return Array.from({ length: channels }, (_, index) => `Canal ${index + 1}`);
}

function parseChannelHandle(handleId: string | null | undefined): number | "all" {
  if (!handleId?.startsWith("ch-")) {
    return "all";
  }
  const channel = Number(handleId.slice(3));
  return Number.isInteger(channel) && channel >= 0 ? channel : "all";
}

function normalizeChannelValue(value: unknown): number | "all" {
  if (value === "all") {
    return "all";
  }
  const channel = Number(value);
  return Number.isInteger(channel) && channel >= 0 ? channel : "all";
}

function normalizeHandleId(handleId: string | null | undefined, channel: unknown) {
  if (handleId === "all" || handleId?.startsWith("ch-")) {
    return handleId;
  }
  const normalizedChannel = normalizeChannelValue(channel);
  return normalizedChannel === "all" ? "all" : `ch-${normalizedChannel}`;
}

function keepChannelLinkedNodesExpanded(nodes: AudioFlowNode[], edges: AudioFlowEdge[]) {
  const channelLinkedNodeIds = new Set<string>();
  for (const edge of edges) {
    if (edge.data?.sourceChannel !== undefined && edge.data.sourceChannel !== "all") {
      channelLinkedNodeIds.add(edge.source);
    }
    if (edge.data?.targetChannel !== undefined && edge.data.targetChannel !== "all") {
      channelLinkedNodeIds.add(edge.target);
    }
  }

  return nodes.map((node) =>
    channelLinkedNodeIds.has(node.id)
      ? {
          ...node,
          data: {
            ...node.data,
            expanded: true,
          },
        }
      : node,
  );
}

function visualGainForNode(nodeId: string, edges: AudioFlowEdge[], activeNodeIds: Set<string>) {
  const gains = edges
    .filter((edge) => edge.source === nodeId || edge.target === nodeId)
    .filter((edge) => activeNodeIds.has(edge.source) && activeNodeIds.has(edge.target))
    .map((edge) => edge.data?.gain ?? 1);
  if (gains.length === 0) {
    return 1;
  }

  return Math.min(4, Math.max(0, Math.max(...gains)));
}
