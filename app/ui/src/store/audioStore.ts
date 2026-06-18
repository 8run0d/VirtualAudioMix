import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";

const DYNAMIC_LATENCY_STORAGE_KEY = "virtual-audio-mix:dynamic-latency-enabled:v1";
const MANUAL_LATENCY_STORAGE_KEY = "virtual-audio-mix:manual-latency-ms:v1";
const DEFAULT_MANUAL_LATENCY_MS = 30;
const MIN_MANUAL_LATENCY_MS = 10;
const MAX_MANUAL_LATENCY_MS = 200;

export type AudioDevice = {
  id: string;
  name: string;
  kind: "input" | "output" | "loopback";
  channels: number;
  sampleRate: number;
  channelNames: string[];
  maxInputChannels: number;
  maxOutputChannels: number;
  supportedSampleRates: number[];
  supportedSampleFormats: string[];
};

export type VirtualDriverDevice = {
  class: string;
  status: string;
  friendlyName: string;
  instanceId: string;
};

export type VirtualDriverStatus = {
  serviceName: string;
  serviceInstalled: boolean;
  serviceRunning: boolean;
  serviceState: string | null;
  driverPath: string | null;
  mediaDeviceOk: boolean;
  inputEndpointOk: boolean;
  outputEndpointOk: boolean;
  apoOk: boolean;
  devices: VirtualDriverDevice[];
};

export type AudioTransportStatus = {
  renderBufferSize: number;
  renderAvailableBytes: number;
  renderTotalBytesWritten: number;
  renderTotalBytesRead: number;
  renderOverflowBytes: number;
  sampleRate: number;
  channels: number;
  bitsPerSample: number;
  blockAlign: number;
  captureBufferSize: number;
  captureAvailableBytes: number;
  captureTotalBytesWritten: number;
  captureTotalBytesRead: number;
  captureOverflowBytes: number;
  captureUnderrunBytes: number;
  captureActiveReaders: number;
  captureMaxReaderAvailableBytes: number;
};

export type AudioRuntimeMetrics = {
  vamCaptureWriterLateAvgUs: number;
  vamCaptureWriterLateMaxUs: number;
  vamCaptureWriterLateSamples: number;
  dynamicLatencyTargetMs: number;
  dynamicLatencyMaxMs: number;
  dynamicLatencyOverflowEvents: number;
  dynamicLatencyAdjustments: number;
  dynamicLatencyEnabled: boolean;
  manualLatencyTargetMs: number;
  manualLatencyMinMs: number;
  manualLatencyMaxMs: number;
  audioVisualizerFftEnabled: boolean;
  audioVisualizerFftLastUs: number;
  audioVisualizerFftFallbacks: number;
};

export type AudioGraphRoute = {
  sourceKind: "inputDevice" | "systemAudio" | "application";
  sourceNodeId?: string;
  sourceName?: string;
  sourceProcessId?: number;
  sourceChannel?: number | "all";
  targetKind: "outputDevice" | "virtualInput";
  targetNodeId?: string;
  targetName: string;
  targetChannel?: number | "all";
  gain: number;
};

export type AudioNodeLevel = {
  nodeId: string;
  level: number;
  bands: number[];
  waveform: number[];
};

export type AudioSession = {
  id: string;
  label: string;
  processId?: number | null;
  level: number;
};

type AudioState = {
  devices: AudioDevice[];
  sessions: AudioSession[];
  virtualDriver: VirtualDriverStatus | null;
  transportStatus: AudioTransportStatus | null;
  runtimeMetrics: AudioRuntimeMetrics | null;
  dynamicLatencyEnabled: boolean;
  manualLatencyMs: number;
  directRouteActive: boolean;
  error: string | null;
  status: string | null;
  refreshDevices: () => Promise<void>;
  refreshAudioSessions: () => Promise<void>;
  refreshVirtualDriver: () => Promise<void>;
  refreshTransportStatus: () => Promise<void>;
  refreshRuntimeMetrics: () => Promise<void>;
  refreshDynamicLatencyEnabled: () => Promise<void>;
  getAudioNodeLevels: () => Promise<AudioNodeLevel[]>;
  setDynamicLatencyEnabled: (enabled: boolean) => Promise<void>;
  setManualLatencyMs: (latencyMs: number) => Promise<number>;
  startDirectRoute: (inputDeviceName: string, outputDeviceName: string) => Promise<boolean>;
  startSystemAudioRoute: (outputDeviceName: string) => Promise<boolean>;
  startAudioGraphRoute: (routes: AudioGraphRoute[]) => Promise<boolean>;
  stopDirectRoute: (message?: string) => Promise<void>;
};

export const useAudioStore = create<AudioState>((set) => ({
  devices: [],
  sessions: [],
  virtualDriver: null,
  transportStatus: null,
  runtimeMetrics: null,
  dynamicLatencyEnabled: readDynamicLatencyEnabled(),
  manualLatencyMs: readManualLatencyMs(),
  directRouteActive: false,
  error: null,
  status: null,
  refreshDevices: async () => {
    try {
      const devices = await invoke<AudioDevice[]>("list_audio_devices");
      set({ devices, error: null });
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
    }
  },
  refreshAudioSessions: async () => {
    try {
      const sessions = await invoke<AudioSession[]>("list_audio_sessions");
      set({ sessions, error: null });
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
    }
  },
  refreshVirtualDriver: async () => {
    try {
      const virtualDriver = await invoke<VirtualDriverStatus>("get_virtual_driver_status");
      set({ virtualDriver, error: null });
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
    }
  },
  refreshTransportStatus: async () => {
    try {
      const transportStatus = await invoke<AudioTransportStatus | null>("get_audio_transport_status");
      set({ transportStatus });
    } catch {
      set({ transportStatus: null });
    }
  },
  refreshRuntimeMetrics: async () => {
    try {
      const runtimeMetrics = await invoke<AudioRuntimeMetrics>("get_audio_runtime_metrics");
      set({ runtimeMetrics });
    } catch {
      set({ runtimeMetrics: null });
    }
  },
  refreshDynamicLatencyEnabled: async () => {
    try {
      const dynamicLatencyEnabled = await invoke<boolean>("get_dynamic_latency_enabled");
      localStorage.setItem(DYNAMIC_LATENCY_STORAGE_KEY, JSON.stringify(dynamicLatencyEnabled));
      set({ dynamicLatencyEnabled });
    } catch {
      set({ dynamicLatencyEnabled: readDynamicLatencyEnabled() });
    }
  },
  getAudioNodeLevels: async () => {
    try {
      return await invoke<AudioNodeLevel[]>("get_audio_node_levels");
    } catch {
      return [];
    }
  },
  setDynamicLatencyEnabled: async (enabled) => {
    localStorage.setItem(DYNAMIC_LATENCY_STORAGE_KEY, JSON.stringify(enabled));
    set({ dynamicLatencyEnabled: enabled });
    try {
      await invoke("set_dynamic_latency_enabled", { enabled });
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
    }
  },
  setManualLatencyMs: async (latencyMs) => {
    const sanitizedLatencyMs = sanitizeManualLatencyMs(latencyMs);
    localStorage.setItem(MANUAL_LATENCY_STORAGE_KEY, JSON.stringify(sanitizedLatencyMs));
    set({ manualLatencyMs: sanitizedLatencyMs });
    try {
      const appliedLatencyMs = await invoke<number>("set_manual_latency_target_ms", {
        latencyMs: sanitizedLatencyMs,
      });
      localStorage.setItem(MANUAL_LATENCY_STORAGE_KEY, JSON.stringify(appliedLatencyMs));
      set({ manualLatencyMs: appliedLatencyMs });
      return appliedLatencyMs;
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
      return sanitizedLatencyMs;
    }
  },
  startDirectRoute: async (inputDeviceName, outputDeviceName) => {
    try {
      await invoke("start_direct_audio_route", { inputDeviceName, outputDeviceName });
      set({
        directRouteActive: true,
        runtimeMetrics: null,
        error: null,
        status: `Routage actif: ${inputDeviceName} -> ${outputDeviceName}`,
      });
      return true;
    } catch (error) {
      set({ directRouteActive: false, error: error instanceof Error ? error.message : String(error), status: null });
      return false;
    }
  },
  startSystemAudioRoute: async (outputDeviceName) => {
    try {
      await invoke("start_system_audio_route", { outputDeviceName });
      set({
        directRouteActive: true,
        runtimeMetrics: null,
        error: null,
        status: `Routage actif: Son système -> ${outputDeviceName}`,
      });
      return true;
    } catch (error) {
      set({ directRouteActive: false, error: error instanceof Error ? error.message : String(error), status: null });
      return false;
    }
  },
  startAudioGraphRoute: async (routes) => {
    try {
      await invoke("start_audio_graph_route", { routes });
      set({
        directRouteActive: true,
        runtimeMetrics: null,
        error: null,
        status: `Routage actif: ${routes.length} lien${routes.length > 1 ? "s" : ""}.`,
      });
      return true;
    } catch (error) {
      set({ directRouteActive: false, error: error instanceof Error ? error.message : String(error), status: null });
      return false;
    }
  },
  stopDirectRoute: async (message) => {
    try {
      await invoke("stop_direct_audio_route");
      set({ directRouteActive: false, runtimeMetrics: null, error: null, status: message ?? "Routage audio arrêté." });
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
    }
  },
}));

function readDynamicLatencyEnabled() {
  try {
    const value = localStorage.getItem(DYNAMIC_LATENCY_STORAGE_KEY);
    return value === null ? true : Boolean(JSON.parse(value));
  } catch {
    return true;
  }
}

function readManualLatencyMs() {
  try {
    const value = localStorage.getItem(MANUAL_LATENCY_STORAGE_KEY);
    return sanitizeManualLatencyMs(value === null ? DEFAULT_MANUAL_LATENCY_MS : Number(JSON.parse(value)));
  } catch {
    return DEFAULT_MANUAL_LATENCY_MS;
  }
}

function sanitizeManualLatencyMs(latencyMs: number) {
  if (!Number.isFinite(latencyMs)) {
    return DEFAULT_MANUAL_LATENCY_MS;
  }
  return Math.round(Math.min(MAX_MANUAL_LATENCY_MS, Math.max(MIN_MANUAL_LATENCY_MS, latencyMs)));
}
