import { BaseEdge, EdgeLabelRenderer, EdgeProps, getBezierPath } from "@xyflow/react";
import { CSSProperties, ChangeEvent, PointerEvent, useRef, useState } from "react";
import { useGraphStore } from "../store/graphStore";

export function Edge({ id, sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, data }: EdgeProps) {
  const setEdgeGain = useGraphStore((state) => state.setEdgeGain);
  const dragRef = useRef<{ pointerId: number; startX: number; startGain: number } | null>(null);
  const [draftGain, setDraftGain] = useState("");
  const [basePath, baseLabelX, baseLabelY] = getBezierPath({
    sourceX,
    sourceY,
    sourcePosition,
    targetX,
    targetY,
    targetPosition,
  });
  const gain = typeof data?.gain === "number" ? data.gain : 1;
  const channelSummary = formatChannelSummary(data?.sourceChannel, data?.targetChannel);
  const labelOffset = edgeLabelOffset(id, sourceX, sourceY, targetX, targetY);
  const labelX = baseLabelX + labelOffset.x;
  const labelY = baseLabelY + labelOffset.y;
  const path =
    labelOffset.x === 0 && labelOffset.y === 0
      ? basePath
      : buildRoutedEdgePath(sourceX, sourceY, targetX, targetY, labelX, labelY);

  const onGainInputChange = (event: ChangeEvent<HTMLInputElement>) => {
    setDraftGain(event.target.value);
  };
  const commitDraftGain = () => {
    if (!draftGain.trim()) {
      setDraftGain("");
      return;
    }

    const parsed = Number(draftGain.replace(",", "."));
    if (Number.isFinite(parsed)) {
      setEdgeGain(id, parsed);
    }
    setDraftGain("");
  };
  const stopPointerPropagation = (event: PointerEvent<HTMLInputElement>) => {
    event.stopPropagation();
  };
  const onKnobPointerDown = (event: PointerEvent<HTMLButtonElement>) => {
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    dragRef.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startGain: gain,
    };
  };
  const onKnobPointerMove = (event: PointerEvent<HTMLButtonElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) {
      return;
    }

    const delta = (event.clientX - drag.startX) / 120;
    setEdgeGain(id, drag.startGain + delta);
  };
  const onKnobPointerUp = (event: PointerEvent<HTMLButtonElement>) => {
    if (dragRef.current?.pointerId === event.pointerId) {
      dragRef.current = null;
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  };

  return (
    <>
      <BaseEdge id={id} path={path} className="audio-edge" />
      <EdgeLabelRenderer>
        <div
          className="edge-label"
          style={{
            transform: `translate(-50%, -50%) translate(${labelX}px,${labelY}px)`,
          }}
        >
          <button
            aria-label="Glisser pour modifier le volume du lien"
            className="edge-gain-knob"
            style={{ "--gain-angle": `${Math.min(300, Math.max(0, gain * 75))}deg` } as CSSProperties}
            title="Maintenir et glisser horizontalement pour régler le volume. Double-clic: 1.00"
            type="button"
            onDoubleClick={() => setEdgeGain(id, 1)}
            onPointerDown={onKnobPointerDown}
            onPointerMove={onKnobPointerMove}
            onPointerUp={onKnobPointerUp}
            onPointerCancel={onKnobPointerUp}
          >
            <span style={{ transform: `rotate(${Math.min(300, Math.max(0, gain * 75)) - 150}deg)` }} />
          </button>
          <input
            aria-label="Volume du lien"
            min="0"
            max="4"
            title="Saisie manuelle du volume du lien"
            type="text"
            inputMode="decimal"
            value={draftGain || gain.toFixed(2)}
            onBlur={commitDraftGain}
            onChange={onGainInputChange}
            onDoubleClick={() => setEdgeGain(id, 1)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.currentTarget.blur();
              }
              if (event.key === "Escape") {
                setDraftGain("");
                event.currentTarget.blur();
              }
            }}
            onPointerDown={stopPointerPropagation}
          />
          {channelSummary ? <span className="edge-channel-summary">{channelSummary}</span> : null}
        </div>
      </EdgeLabelRenderer>
    </>
  );
}

function formatChannelSummary(sourceChannel: unknown, targetChannel: unknown) {
  const source = formatChannel(sourceChannel);
  const target = formatChannel(targetChannel);
  if (!source && !target) {
    return "";
  }
  return `${source || "all"} → ${target || "all"}`;
}

function formatChannel(channel: unknown) {
  if (channel === undefined || channel === null || channel === "all") {
    return "";
  }
  if (channel === 0) {
    return "L";
  }
  if (channel === 1) {
    return "R";
  }
  if (typeof channel === "number" && Number.isFinite(channel)) {
    return `C${channel + 1}`;
  }
  return String(channel);
}

function edgeLabelOffset(edgeId: string, sourceX: number, sourceY: number, targetX: number, targetY: number) {
  const hash = Array.from(edgeId).reduce((total, char) => total + char.charCodeAt(0), 0);
  const lane = (hash % 5) - 2;
  if (lane === 0) {
    return { x: 0, y: 0 };
  }

  const dx = targetX - sourceX;
  const dy = targetY - sourceY;
  const length = Math.hypot(dx, dy) || 1;
  if (length < 190 && Math.abs(dy) < 70) {
    return { x: 0, y: 0 };
  }
  const normalX = -dy / length;
  const normalY = dx / length;
  const distance = lane * 18;
  return {
    x: normalX * distance,
    y: normalY * distance,
  };
}

function buildRoutedEdgePath(
  sourceX: number,
  sourceY: number,
  targetX: number,
  targetY: number,
  waypointX: number,
  waypointY: number,
) {
  const controlOffset = Math.max(80, Math.abs(targetX - sourceX) * 0.42);
  const curveBiasX = (waypointX - (sourceX + targetX) * 0.5) * 0.68;
  const curveBiasY = (waypointY - (sourceY + targetY) * 0.5) * 1.65;
  const control1X = sourceX + controlOffset + curveBiasX;
  const control2X = targetX - controlOffset + curveBiasX;
  const control1Y = sourceY + curveBiasY;
  const control2Y = targetY + curveBiasY;

  return `M ${sourceX},${sourceY} C ${control1X},${control1Y} ${control2X},${control2Y} ${targetX},${targetY}`;
}
