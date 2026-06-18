import { Handle, NodeProps, Position, useUpdateNodeInternals } from "@xyflow/react";
import { AppWindow, ChevronDown, ChevronRight, Mic, MonitorSpeaker, Volume2 } from "lucide-react";
import { useEffect } from "react";
import { AudioFlowNode, useGraphStore } from "../store/graphStore";

const icons = {
  inputDevice: Mic,
  outputDevice: Volume2,
  application: AppWindow,
  systemAudio: MonitorSpeaker,
  virtualOutput: Volume2,
};

export function Node({ id, data }: NodeProps<AudioFlowNode>) {
  const updateNodeInternals = useUpdateNodeInternals();
  const Icon = icons[data.kind];
  const toggleNodeEnabled = useGraphStore((state) => state.toggleNodeEnabled);
  const toggleNodeExpanded = useGraphStore((state) => state.toggleNodeExpanded);
  const enabled = data.enabled !== false;
  const channels = sanitizeChannelCount(data.channels);
  const channelNames = normalizeChannelNames(channels, data.channelNames, data.kind);
  const canExpand = channels > 1;
  const expanded = canExpand && data.expanded === true;
  const virtualInput = isVirtualInputNode(data.label);
  const acceptsInput = data.kind === "outputDevice" || virtualInput;
  const emitsOutput =
    (data.kind === "inputDevice" && !virtualInput) ||
    data.kind === "systemAudio" ||
    data.kind === "virtualOutput" ||
    data.kind === "application";
  const channelHandleType = acceptsInput ? "target" : "source";
  const visualGain = Math.min(4, Math.max(0, Number(data.visualGain ?? 1)));
  const level = enabled ? Math.min(1, data.level * visualGain) : 0;
  const bands = enabled ? normalizeBands(data.bands).map((band) => Math.min(1, band * visualGain)) : [0, 0, 0, 0, 0, 0, 0, 0];
  const waveform = enabled ? normalizeWaveform(data.waveform).map((point) => Math.min(1, Math.max(-1, point * visualGain))) : zeroWaveform();
  const wavePaths = buildWavePaths(level, bands, waveform);
  const deviceMissing = data.deviceMissing === true;
  const rootClassName = ["audio-node", expanded ? "expanded" : "", enabled ? "" : "disabled", deviceMissing ? "missing" : ""]
    .filter(Boolean)
    .join(" ");

  useEffect(() => {
    requestAnimationFrame(() => updateNodeInternals(id));
  }, [channels, expanded, id, updateNodeInternals]);

  return (
    <div className={rootClassName}>
      {acceptsInput && !expanded ? (
        <Handle className="node-handle node-main-handle" id="all" type="target" position={Position.Left} title="Tous les canaux" />
      ) : null}
      <div className="node-title">
        <Icon size={16} />
        <span>{data.label}</span>
        {deviceMissing ? <span className="node-missing-badge">absent</span> : null}
        {canExpand ? (
          <button
            className="node-expand-button"
            title={expanded ? "Masquer les sous-canaux" : "Afficher les sous-canaux"}
            type="button"
            onClick={(event) => {
              event.stopPropagation();
              toggleNodeExpanded(id);
            }}
          >
            {expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
          </button>
        ) : null}
        <button
          className={enabled ? "node-switch active" : "node-switch"}
          title={enabled ? "Désactiver ce bloc" : "Activer ce bloc"}
          type="button"
          onClick={(event) => {
            event.stopPropagation();
            toggleNodeEnabled(id);
          }}
        >
          <span />
        </button>
      </div>
      <div className="node-waveform" aria-label="Indicateur visuel de niveau audio">
        <div className="node-waveform-frame">
          <svg viewBox="0 0 80 36" preserveAspectRatio="none" aria-hidden="true">
            <path className="waveform-axis" d="M1 18 C 20 18, 60 18, 79 18" />
            {wavePaths.map((path, index) => (
              <g key={index} className={path.primary ? "waveform-group primary" : "waveform-group"}>
                <path className="waveform-line upper" d={path.upper} style={{ opacity: enabled ? path.opacity : 0.12 }} />
                <path className="waveform-line lower" d={path.lower} style={{ opacity: enabled ? path.opacity * 0.72 : 0.1 }} />
              </g>
            ))}
          </svg>
        </div>
      </div>
      {expanded ? (
        <div className="node-channels" aria-label="Sous-canaux disponibles">
          <div className={`node-channel-row node-channel-all ${channelHandleType}`}>
            {channelHandleType === "target" ? (
              <Handle
                className="node-channel-handle react-flow-channel-handle"
                id="all"
                type="target"
                position={Position.Left}
                title="Tous les canaux"
              />
            ) : (
              <Handle
                className="node-channel-handle react-flow-channel-handle"
                id="all"
                type="source"
                position={Position.Right}
                title="Tous les canaux"
              />
            )}
            <span className="node-channel-name">ALL</span>
          </div>
          {channelNames.map((channelName, index) => (
            <div key={`${channelName}-${index}`} className={`node-channel-row ${channelHandleType}`}>
              {channelHandleType === "target" ? (
                <Handle
                  className="node-channel-handle react-flow-channel-handle"
                  id={`ch-${index}`}
                  type="target"
                  position={Position.Left}
                  title={`Canal ${channelName}`}
                />
              ) : (
                <Handle
                  className="node-channel-handle react-flow-channel-handle"
                  id={`ch-${index}`}
                  type="source"
                  position={Position.Right}
                  title={`Canal ${channelName}`}
                />
              )}
              <span className="node-channel-name">{channelName}</span>
            </div>
          ))}
        </div>
      ) : null}
      {emitsOutput && !expanded ? (
        <Handle className="node-handle node-main-handle" id="all" type="source" position={Position.Right} title="Tous les canaux" />
      ) : null}
    </div>
  );
}

function buildWavePaths(level: number, bands: number[], waveform: number[]) {
  const width = 80;
  const center = 18;
  const responsiveLevel = Math.pow(Math.min(1, Math.max(0, level)), 0.52);
  const safeBands = normalizeBands(bands);
  const safeWaveform = normalizeWaveform(waveform);
  const spectralEnergy = safeBands.reduce((total, band) => total + band, 0) / safeBands.length;
  const visibleWaveform = safeWaveform.map((point, index) => {
    const normalized = index / Math.max(1, safeWaveform.length - 1);
    const envelope = 0.28 + 0.72 * Math.sin(Math.PI * normalized);
    return point * envelope * Math.max(0.12, responsiveLevel + spectralEnergy * 0.3);
  });

  const detailPaths = [0.72, 0.9, 1.08, 1.24].map((scale, layer) => {
    const bandBias = safeBands[layer + 2] ?? spectralEnergy;
    return {
      ...buildDoubledWaveformPaths(visibleWaveform, width, center, scale + bandBias * 0.34),
      opacity: 0.13 + Math.max(responsiveLevel, bandBias) * 0.32,
      primary: false,
    };
  });

  return [
    {
      ...buildDoubledWaveformPaths(visibleWaveform, width, center, 1.0),
      opacity: 0.24 + responsiveLevel * 0.72,
      primary: true,
    },
    ...detailPaths,
  ];
}

function buildDoubledWaveformPaths(waveform: number[], width: number, center: number, scale: number) {
  return {
    upper: buildWaveformPath(waveform, width, center - 0.45, scale, 0),
    lower: buildWaveformPath(
      waveform.map((point) => point * 0.86),
      width,
      center + 1.65,
      scale,
      1.35,
    ),
  };
}

function buildWaveformPath(waveform: number[], width: number, center: number, scale: number, xOffset: number) {
  const amplitude = 14;
  const points = waveform.map((point, index) => {
    const x = (index / Math.max(1, waveform.length - 1)) * width + xOffset;
    const y = center - point * amplitude * scale;
    return { x, y };
  });
  return buildLinearPath(points);
}

function buildLinearPath(points: Array<{ x: number; y: number }>) {
  return points
    .map((point, index) => {
      if (index === 0) {
        return `M ${point.x.toFixed(1)} ${point.y.toFixed(1)}`;
      }
      return `L ${point.x.toFixed(1)} ${point.y.toFixed(1)}`;
    })
    .join(" ");
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

function sanitizeChannelCount(value: unknown) {
  const channels = Number(value ?? 1);
  return Number.isFinite(channels) ? Math.min(16, Math.max(1, Math.floor(channels))) : 1;
}

function normalizeChannelNames(channels: number, value: unknown, kind: unknown) {
  const names = Array.isArray(value) ? value.map((name) => String(name)) : [];
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

function isVirtualInputNode(label: unknown) {
  const value = String(label ?? "");
  return value.includes("VAM Entrée") || value.includes("VAM IN");
}
