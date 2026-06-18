import { ChangeEvent, FormEvent, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useReactFlow, type XYPosition } from "@xyflow/react";
import {
  Cable,
  CheckCircle2,
  Download,
  Info,
  Play,
  Plus,
  RefreshCw,
  Settings,
  SlidersHorizontal,
  Square,
  Upload,
  XCircle,
} from "lucide-react";
import { GraphCanvas } from "./components/GraphCanvas";
import {
  type AudioDevice,
  type AudioGraphRoute,
  type AudioRuntimeMetrics,
  type AudioSession,
  type AudioTransportStatus,
  useAudioStore,
} from "./store/audioStore";
import { type AudioFlowEdge, type AudioFlowNode, type AudioNodeKind, useGraphStore } from "./store/graphStore";

const ESTIMATED_NODE_WIDTH = 320;
const ESTIMATED_NODE_HEIGHT = 120;
const NODE_GAP_X = 34;
const NODE_GAP_Y = 28;
const CANVAS_MARGIN = 36;
const USER_AUDIO_DEFAULTS_STORAGE_KEY = "virtual-audio-mix:user-audio-defaults:v1";
const USER_APP_PREFERENCES_STORAGE_KEY = "virtual-audio-mix:app-preferences:v1";
const USER_WINDOWS_STARTUP_INITIALIZED_KEY = "virtual-audio-mix:windows-startup-initialized:v1";
const USER_INSTALLER_PREFERENCES_APPLIED_KEY = "virtual-audio-mix:installer-preferences-applied:v1";
const USER_PRESETS_STORAGE_KEY = "virtual-audio-mix:user-presets:v1";
const USER_LAST_PRESET_STORAGE_KEY = "virtual-audio-mix:last-preset:v1";
const NOTICE_TIMEOUT_MS = 5_000;

type UserAudioDefaults = {
  inputDeviceName: string;
  outputDeviceName: string;
};

type UserAppPreferences = {
  autoStartAudio: boolean;
};

type InstallerPreferences = {
  startWithWindows?: boolean | null;
  autoStartAudio?: boolean | null;
  promptAudioSetup?: boolean | null;
};

type UserPreset = {
  id: string;
  name: string;
  createdAt: string;
  nodes: AudioFlowNode[];
  edges: AudioFlowEdge[];
};

type PresetBundle = {
  version: 1;
  exportedAt: string;
  presets: UserPreset[];
};

type WritableFileHandle = {
  createWritable: () => Promise<{
    write: (data: Blob | string) => Promise<void>;
    close: () => Promise<void>;
  }>;
};

type WritableDirectoryHandle = {
  name: string;
  getFileHandle: (name: string, options: { create: boolean }) => Promise<WritableFileHandle>;
};

type GraphRouteDescriptor = {
  edgeId: string;
  sourceNodeId: string;
  targetNodeId: string;
  route: AudioGraphRoute;
};

type DiagnosticHistoryEntry = {
  at: number;
  captureUnderrunBytes: number;
  captureOverflowBytes: number;
  renderOverflowBytes: number;
  captureMs: number;
  renderMs: number;
  writerLateMaxUs: number;
};

type AudioDevicesChangedPayload = {
  devices: AudioDevice[];
};

export function App() {
  const { screenToFlowPosition } = useReactFlow<AudioFlowNode>();
  const [activeGraphRoutes, setActiveGraphRoutes] = useState<
    {
      edgeId: string;
      sourceNodeId: string;
      targetNodeId: string;
    }[]
  >([]);
  const [activeGraphSignature, setActiveGraphSignature] = useState("");
  const [defaultsDialogOpen, setDefaultsDialogOpen] = useState(false);
  const [aboutDialogOpen, setAboutDialogOpen] = useState(false);
  const [selectedDefaultInput, setSelectedDefaultInput] = useState("");
  const [selectedDefaultOutput, setSelectedDefaultOutput] = useState("");
  const [defaultRoutingApplied, setDefaultRoutingApplied] = useState(false);
  const [installerPreferencesLoaded, setInstallerPreferencesLoaded] = useState(false);
  const [promptAudioSetup, setPromptAudioSetup] = useState(true);
  const [autoStartAudio, setAutoStartAudio] = useState(() => readUserAppPreferences().autoStartAudio);
  const [startWithWindowsEnabled, setStartWithWindowsEnabled] = useState(true);
  const [startWithWindowsBusy, setStartWithWindowsBusy] = useState(false);
  const [startupProgressVisible, setStartupProgressVisible] = useState(true);
  const [startupProgress, setStartupProgress] = useState(12);
  const [startupMessage, setStartupMessage] = useState("Chargement du driver BAD...");
  const [presets, setPresets] = useState<UserPreset[]>(() => readUserPresets());
  const [selectedPresetId, setSelectedPresetId] = useState(() => readLastPresetId());
  const [detailsDialogOpen, setDetailsDialogOpen] = useState(false);
  const [diagnosticHistory, setDiagnosticHistory] = useState<DiagnosticHistoryEntry[]>([]);
  const [uiNotice, setUiNotice] = useState("");
  const [presetExportMenuOpen, setPresetExportMenuOpen] = useState(false);
  const [presetExportDirectory, setPresetExportDirectory] = useState<WritableDirectoryHandle | null>(null);
  const [presetExportDirectoryName, setPresetExportDirectoryName] = useState("Téléchargements");
  const presetImportInputRef = useRef<HTMLInputElement | null>(null);
  const autoStartAttemptedRef = useRef(false);
  const pendingTrayPresetStartRef = useRef<string | null>(null);
  const presetsRef = useRef<UserPreset[]>([]);
  const directRouteActiveRef = useRef(false);
  const graphRoutesRef = useRef<GraphRouteDescriptor[]>([]);
  const graphSignatureRef = useRef("");
  const addNode = useGraphStore((state) => state.addNode);
  const applyDefaultRouting = useGraphStore((state) => state.applyDefaultRouting);
  const replaceGraph = useGraphStore((state) => state.replaceGraph);
  const markDeviceAvailability = useGraphStore((state) => state.markDeviceAvailability);
  const nodes = useGraphStore((state) => state.nodes);
  const edges = useGraphStore((state) => state.edges);
  const devices = useAudioStore((state) => state.devices);
  const sessions = useAudioStore((state) => state.sessions);
  const virtualDriver = useAudioStore((state) => state.virtualDriver);
  const transportStatus = useAudioStore((state) => state.transportStatus);
  const runtimeMetrics = useAudioStore((state) => state.runtimeMetrics);
  const dynamicLatencyEnabled = useAudioStore((state) => state.dynamicLatencyEnabled);
  const manualLatencyMs = useAudioStore((state) => state.manualLatencyMs);
  const [pendingManualLatencyMs, setPendingManualLatencyMs] = useState(manualLatencyMs);
  const directRouteActive = useAudioStore((state) => state.directRouteActive);
  const audioError = useAudioStore((state) => state.error);
  const audioStatus = useAudioStore((state) => state.status);
  const refreshDevices = useAudioStore((state) => state.refreshDevices);
  const refreshAudioSessions = useAudioStore((state) => state.refreshAudioSessions);
  const refreshVirtualDriver = useAudioStore((state) => state.refreshVirtualDriver);
  const refreshTransportStatus = useAudioStore((state) => state.refreshTransportStatus);
  const refreshRuntimeMetrics = useAudioStore((state) => state.refreshRuntimeMetrics);
  const setDynamicLatencyEnabled = useAudioStore((state) => state.setDynamicLatencyEnabled);
  const setManualLatencyMs = useAudioStore((state) => state.setManualLatencyMs);
  const startAudioGraphRoute = useAudioStore((state) => state.startAudioGraphRoute);
  const stopDirectRoute = useAudioStore((state) => state.stopDirectRoute);
  const inputDevices = useMemo(
    () => devices.filter((device) => device.kind === "input" || device.kind === "loopback"),
    [devices],
  );
  const outputDevices = useMemo(() => devices.filter((device) => device.kind === "output"), [devices]);
  const selectableDefaultInputs = useMemo(() => {
    const physicalInputs = devices.filter((device) => device.kind === "input" && !isVirtualInputNode(device.name));
    return physicalInputs.length > 0 ? physicalInputs : inputDevices;
  }, [devices, inputDevices]);
  const selectableDefaultOutputs = useMemo(() => {
    const physicalOutputs = outputDevices.filter((device) => !isVirtualOutputNode(device.name));
    return physicalOutputs.length > 0 ? physicalOutputs : outputDevices;
  }, [outputDevices]);
  const driverReady =
    virtualDriver?.serviceRunning &&
    virtualDriver.mediaDeviceOk &&
    virtualDriver.inputEndpointOk &&
    virtualDriver.outputEndpointOk;
  const audioDeviceNamesSignature = useMemo(
    () => devices.map((device) => `${device.kind}:${device.name}:${device.channels}`).sort().join("|"),
    [devices],
  );
  const audioSessionSignature = useMemo(
    () => sessions.map((session) => `${session.id}:${session.label}:${session.processId ?? ""}`).sort().join("|"),
    [sessions],
  );
  const availableResourceNames = useMemo(
    () => [...devices.map((device) => device.name), ...sessions.map((session) => session.label)],
    [audioDeviceNamesSignature, audioSessionSignature],
  );
  const graphDefinitionSignature = useMemo(() => createGraphDefinitionSignature(nodes, edges), [edges, nodes]);
  const graphRoutes = useMemo(
    () => buildGraphRoutes(nodes, edges, devices, sessions),
    [audioDeviceNamesSignature, audioSessionSignature, graphDefinitionSignature],
  );
  const graphSignature = useMemo(() => createGraphRouteSignature(graphRoutes), [graphRoutes]);
  const missingDeviceLabels = useMemo(
    () => findMissingDeviceLabels(nodes, devices, sessions),
    [audioDeviceNamesSignature, audioSessionSignature, nodes],
  );

  const addNodeInView = (
    kind: AudioNodeKind,
    label?: string,
    options?: { channels?: number; channelNames?: string[]; expanded?: boolean; processId?: number | null },
  ) => {
    addNode(kind, label, getVisibleNodePosition(nodes, screenToFlowPosition), options);
  };

  useEffect(() => {
    void refreshDevices();
    void refreshAudioSessions();
    void refreshVirtualDriver();
    void refreshTransportStatus();
    void refreshRuntimeMetrics();
    void setDynamicLatencyEnabled(dynamicLatencyEnabled);
    void setManualLatencyMs(manualLatencyMs);
  }, [refreshAudioSessions, refreshDevices, refreshRuntimeMetrics, refreshTransportStatus, refreshVirtualDriver]);

  useEffect(() => {
    let disposed = false;
    const initializeInstallerPreferences = async () => {
      try {
        const [enabled, installerPreferences] = await Promise.all([
          invoke<boolean>("get_start_with_windows"),
          invoke<InstallerPreferences>("get_installer_preferences").catch((): InstallerPreferences => ({})),
        ]);
        if (disposed) {
          return;
        }

        const firstInstallerRun = !localStorage.getItem(USER_INSTALLER_PREFERENCES_APPLIED_KEY);
        const firstWindowsStartupRun = !localStorage.getItem(USER_WINDOWS_STARTUP_INITIALIZED_KEY);
        const desiredStartWithWindows =
          firstInstallerRun && typeof installerPreferences.startWithWindows === "boolean"
            ? installerPreferences.startWithWindows
            : firstWindowsStartupRun
              ? true
              : enabled;

        if (firstWindowsStartupRun) {
          localStorage.setItem(USER_WINDOWS_STARTUP_INITIALIZED_KEY, "1");
        }

        if (enabled !== desiredStartWithWindows) {
          await invoke("set_start_with_windows", { enabled: desiredStartWithWindows });
        }
        setStartWithWindowsEnabled(desiredStartWithWindows);

        if (firstInstallerRun) {
          if (typeof installerPreferences.autoStartAudio === "boolean") {
            setAutoStartAudio(installerPreferences.autoStartAudio);
            writeUserAppPreferences({ autoStartAudio: installerPreferences.autoStartAudio });
          }
          if (typeof installerPreferences.promptAudioSetup === "boolean") {
            setPromptAudioSetup(installerPreferences.promptAudioSetup);
          }
          localStorage.setItem(USER_INSTALLER_PREFERENCES_APPLIED_KEY, "1");
        }
      } catch {
        if (!disposed) {
          setStartWithWindowsEnabled(true);
        }
      } finally {
        if (!disposed) {
          setInstallerPreferencesLoaded(true);
        }
      }
    };

    void initializeInstallerPreferences();
    return () => {
      disposed = true;
    };
  }, []);

  useEffect(() => {
    void invoke("update_tray_presets", {
      presets: presets.map((preset) => ({ id: preset.id, name: preset.name })),
    });
  }, [presets]);

  useEffect(() => {
    presetsRef.current = presets;
  }, [presets]);

  useEffect(() => {
    directRouteActiveRef.current = directRouteActive;
  }, [directRouteActive]);

  useEffect(() => {
    graphRoutesRef.current = graphRoutes;
    graphSignatureRef.current = graphSignature;
  }, [graphRoutes, graphSignature]);

  useEffect(() => {
    const unlistenPromise = listen<AudioDevicesChangedPayload>("audio-devices-changed", (event) => {
      useAudioStore.setState({ devices: event.payload.devices, error: null });
      void refreshVirtualDriver();
      void refreshTransportStatus();
    });

    return () => {
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, [refreshTransportStatus, refreshVirtualDriver]);

  useEffect(() => {
    markDeviceAvailability(availableResourceNames);
  }, [availableResourceNames, markDeviceAvailability]);

  useEffect(() => {
    const timer = window.setInterval(() => {
      void refreshAudioSessions();
    }, 2_000);

    return () => window.clearInterval(timer);
  }, [refreshAudioSessions]);

  useEffect(() => {
    setPendingManualLatencyMs(manualLatencyMs);
  }, [manualLatencyMs]);

  useEffect(() => {
    if (!uiNotice) {
      return;
    }

    const timer = window.setTimeout(() => setUiNotice(""), NOTICE_TIMEOUT_MS);
    return () => window.clearTimeout(timer);
  }, [uiNotice]);

  useEffect(() => {
    if (!installerPreferencesLoaded || defaultRoutingApplied || devices.length === 0) {
      return;
    }

    const lastPresetId = readLastPresetId();
    const lastPreset = presets.find((preset) => preset.id === lastPresetId);
    if (lastPreset) {
      setSelectedPresetId(lastPreset.id);
      replaceGraph(lastPreset.nodes, lastPreset.edges);
      setDefaultRoutingApplied(true);
      return;
    }

    const savedDefaults = readUserAudioDefaults();
    const savedInputAvailable = savedDefaults
      ? selectableDefaultInputs.some((device) => device.name === savedDefaults.inputDeviceName)
      : false;
    const savedOutputAvailable = savedDefaults
      ? selectableDefaultOutputs.some((device) => device.name === savedDefaults.outputDeviceName)
      : false;

    if (savedDefaults && savedInputAvailable && savedOutputAvailable) {
      applyDefaultRouting(savedDefaults.inputDeviceName, savedDefaults.outputDeviceName);
      setDefaultRoutingApplied(true);
      return;
    }

    if (promptAudioSetup) {
      setSelectedDefaultInput(savedDefaults?.inputDeviceName ?? selectableDefaultInputs[0]?.name ?? "");
      setSelectedDefaultOutput(savedDefaults?.outputDeviceName ?? selectableDefaultOutputs[0]?.name ?? "");
      setDefaultsDialogOpen(true);
    } else {
      setDefaultRoutingApplied(true);
    }
  }, [
    applyDefaultRouting,
    defaultRoutingApplied,
    devices.length,
    installerPreferencesLoaded,
    promptAudioSetup,
    presets,
    replaceGraph,
    selectableDefaultInputs,
    selectableDefaultOutputs,
  ]);

  useEffect(() => {
    if (presets.length === 0) {
      if (selectedPresetId) {
        setSelectedPresetId("");
      }
      return;
    }

    if (!presets.some((preset) => preset.id === selectedPresetId)) {
      setSelectedPresetId(presets[0].id);
    }
  }, [presets, selectedPresetId]);

  useEffect(() => {
    if (!directRouteActive) {
      void refreshTransportStatus();
      void refreshRuntimeMetrics();
      return;
    }

    const timer = window.setInterval(() => {
      void refreshTransportStatus();
      void refreshRuntimeMetrics();
    }, 1_000);

    return () => window.clearInterval(timer);
  }, [directRouteActive, refreshRuntimeMetrics, refreshTransportStatus]);

  useEffect(() => {
    if (!transportStatus) {
      return;
    }

    const captureMs = bytesToMs(
      transportStatus.captureMaxReaderAvailableBytes,
      transportStatus.sampleRate,
      transportStatus.channels,
      transportStatus.bitsPerSample,
    );
    const renderMs = bytesToMs(
      transportStatus.renderAvailableBytes,
      transportStatus.sampleRate,
      transportStatus.channels,
      transportStatus.bitsPerSample,
    );
    const entry: DiagnosticHistoryEntry = {
      at: Date.now(),
      captureUnderrunBytes: transportStatus.captureUnderrunBytes,
      captureOverflowBytes: transportStatus.captureOverflowBytes,
      renderOverflowBytes: transportStatus.renderOverflowBytes,
      captureMs,
      renderMs,
      writerLateMaxUs: runtimeMetrics?.vamCaptureWriterLateMaxUs ?? 0,
    };
    setDiagnosticHistory((history) => [...history.slice(-59), entry]);
  }, [runtimeMetrics, transportStatus]);

  useEffect(() => {
    if (!directRouteActive) {
      if (activeGraphRoutes.length > 0) {
        setActiveGraphRoutes([]);
      }
      if (activeGraphSignature) {
        setActiveGraphSignature("");
      }
      return;
    }

    if (!activeGraphSignature || graphSignature === activeGraphSignature) {
      return;
    }

    const timer = window.setTimeout(() => {
      if (graphRoutes.length === 0) {
        setActiveGraphRoutes([]);
        setActiveGraphSignature("");
        void stopDirectRoute("Routage arrêté: aucun lien audio valide dans le graphe.");
        return;
      }

      void startAudioGraphRoute(graphRoutes.map((route) => route.route)).then((started) => {
        if (started) {
          setActiveGraphRoutes(toActiveGraphRoutes(graphRoutes));
          setActiveGraphSignature(graphSignature);
        }
      });
    }, 180);

    return () => window.clearTimeout(timer);
  }, [
    activeGraphRoutes.length,
    activeGraphSignature,
    directRouteActive,
    graphSignature,
    startAudioGraphRoute,
    stopDirectRoute,
  ]);

  useEffect(() => {
    if (!startupProgressVisible) {
      return;
    }

    if (!virtualDriver) {
      setStartupProgress(18);
      setStartupMessage("Chargement du driver BAD...");
      return;
    }

    if (!driverReady) {
      setStartupProgress(100);
      setStartupMessage("Driver BAD indisponible ou incomplet.");
      const timer = window.setTimeout(() => setStartupProgressVisible(false), 2500);
      return () => window.clearTimeout(timer);
    }

    if (!autoStartAudio) {
      setStartupProgress(100);
      setStartupMessage("Driver BAD prêt.");
      const timer = window.setTimeout(() => setStartupProgressVisible(false), 1200);
      return () => window.clearTimeout(timer);
    }

    if (!defaultRoutingApplied) {
      setStartupProgress(72);
      setStartupMessage("Driver BAD prêt. Préparation du graphe audio...");
      return;
    }

    if (directRouteActive) {
      setStartupProgress(100);
      setStartupMessage("Audio démarré.");
      const timer = window.setTimeout(() => setStartupProgressVisible(false), 1200);
      return () => window.clearTimeout(timer);
    }

    if (graphRoutes.length === 0) {
      setStartupProgress(100);
      setStartupMessage("Driver BAD prêt. Aucun routage valide à démarrer.");
      const timer = window.setTimeout(() => setStartupProgressVisible(false), 1800);
      return () => window.clearTimeout(timer);
    }
  }, [autoStartAudio, defaultRoutingApplied, directRouteActive, driverReady, graphRoutes.length, startupProgressVisible, virtualDriver]);

  const stopSelectedRoute = async () => {
    setActiveGraphRoutes([]);
    setActiveGraphSignature("");
    await stopDirectRoute();
  };

  const saveUserDefaults = (event: FormEvent) => {
    event.preventDefault();
    const inputDeviceName = selectedDefaultInput.trim();
    const outputDeviceName = selectedDefaultOutput.trim();

    if (!inputDeviceName || !outputDeviceName) {
      return;
    }

    const defaults = { inputDeviceName, outputDeviceName };
    localStorage.setItem(USER_AUDIO_DEFAULTS_STORAGE_KEY, JSON.stringify(defaults));
    applyDefaultRouting(inputDeviceName, outputDeviceName);
    setDefaultRoutingApplied(true);
    setDefaultsDialogOpen(false);
  };

  const updateAutoStartAudio = (enabled: boolean) => {
    setAutoStartAudio(enabled);
    writeUserAppPreferences({ autoStartAudio: enabled });
  };

  const updateStartWithWindows = async (enabled: boolean) => {
    setStartWithWindowsBusy(true);
    try {
      await invoke("set_start_with_windows", { enabled });
      setStartWithWindowsEnabled(enabled);
      setUiNotice(enabled ? "Démarrage avec Windows activé." : "Démarrage avec Windows désactivé.");
    } catch (error) {
      setUiNotice(`Impossible de modifier le démarrage Windows: ${String(error)}`);
    } finally {
      setStartWithWindowsBusy(false);
    }
  };

  const openAudioDefaultsSettings = () => {
    const savedDefaults = readUserAudioDefaults();
    setSelectedDefaultInput(savedDefaults?.inputDeviceName ?? selectableDefaultInputs[0]?.name ?? "");
    setSelectedDefaultOutput(savedDefaults?.outputDeviceName ?? selectableDefaultOutputs[0]?.name ?? "");
    setDefaultsDialogOpen(true);
  };
  const saveCurrentPreset = () => {
    const fallbackName = `Preset ${presets.length + 1}`;
    const name = window.prompt("Nom du preset", fallbackName)?.trim();
    if (!name) {
      return;
    }

    const preset: UserPreset = {
      id: crypto.randomUUID(),
      name,
      createdAt: new Date().toISOString(),
      nodes: serializePresetNodes(nodes),
      edges: serializePresetEdges(edges),
    };
    const nextPresets = [...presets, preset];
    setPresets(nextPresets);
    setSelectedPresetId(preset.id);
    writeUserPresets(nextPresets);
    writeLastPresetId(preset.id);
    setUiNotice(`Preset sauvegardé: ${preset.name}`);
  };
  const exportJsonFile = async (filename: string, content: string, successMessage: string) => {
    if (presetExportDirectory) {
      try {
        await writeJsonToDirectory(presetExportDirectory, filename, content);
        setUiNotice(`${successMessage} dans ${presetExportDirectory.name}`);
        return;
      } catch {
        setUiNotice("Écriture dans le dossier choisi impossible. Export vers Téléchargements.");
      }
    }

    downloadJsonFile(filename, content);
    setUiNotice(`${successMessage}. Fichier envoyé dans Téléchargements.`);
  };
  const exportSelectedPreset = () => {
    const preset = presets.find((item) => item.id === selectedPresetId);
    if (!preset) {
      window.alert("Sélectionne d'abord un preset à exporter.");
      return;
    }

    const filename = `${safeFilename(preset.name)}.vam-preset.json`;
    setPresetExportMenuOpen(false);
    void exportJsonFile(filename, JSON.stringify(preset, null, 2), `Preset exporté: ${filename}`);
  };
  const exportAllPresets = () => {
    if (presets.length === 0) {
      window.alert("Aucun preset à exporter.");
      return;
    }

    const bundle: PresetBundle = {
      version: 1,
      exportedAt: new Date().toISOString(),
      presets: presets.map((preset) => normalizeUserPreset(preset)).filter((preset): preset is UserPreset => preset !== null),
    };
    const date = new Date().toISOString().slice(0, 10);
    const filename = `VirtualAudioMix-presets-${date}.vam-presets.json`;
    setPresetExportMenuOpen(false);
    void exportJsonFile(filename, JSON.stringify(bundle, null, 2), `${bundle.presets.length} presets exportés: ${filename}`);
  };
  const choosePresetExportDirectory = async () => {
    const picker = getDirectoryPicker();
    if (!picker) {
      setUiNotice("Choix de dossier indisponible ici. Export vers Téléchargements.");
      return;
    }

    try {
      const directory = await picker({ mode: "readwrite" });
      setPresetExportDirectory(directory);
      setPresetExportDirectoryName(directory.name);
      setUiNotice(`Dossier d'export sélectionné: ${directory.name}`);
    } catch {
      setUiNotice("Choix du dossier annulé.");
    }
  };
  const importPresetFile = async (event: ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(event.target.files ?? []);
    event.target.value = "";
    if (files.length === 0) {
      return;
    }

    try {
      const importedPresets: UserPreset[] = [];
      for (const file of files) {
        const imported = JSON.parse(await file.text());
        importedPresets.push(...normalizePresetImport(imported, file.name));
      }

      if (importedPresets.length === 0) {
        window.alert("Preset invalide.");
        return;
      }

      const importedIds = new Set(importedPresets.map((preset) => preset.id));
      const nextPresets = [...presets.filter((item) => !importedIds.has(item.id)), ...importedPresets];
      setPresets(nextPresets);
      setSelectedPresetId(importedPresets[0].id);
      setPresetExportMenuOpen(false);
      writeUserPresets(nextPresets);
      setUiNotice(
        importedPresets.length === 1
          ? `Preset importé: ${importedPresets[0].name}`
          : `${importedPresets.length} presets importés.`,
      );
    } catch {
      window.alert("Impossible d'importer ce ou ces presets JSON.");
    }
  };
  const loadSelectedPreset = () => {
    const preset = presets.find((item) => item.id === selectedPresetId);
    if (!preset) {
      return;
    }
    replaceGraph(preset.nodes, preset.edges);
    writeLastPresetId(preset.id);
    setUiNotice(`Preset chargé: ${preset.name}`);
  };
  const deleteSelectedPreset = () => {
    if (!selectedPresetId) {
      return;
    }
    const nextPresets = presets.filter((preset) => preset.id !== selectedPresetId);
    setPresets(nextPresets);
    setPresetExportMenuOpen(false);
    const nextSelectedPresetId = nextPresets[0]?.id ?? "";
    setSelectedPresetId(nextSelectedPresetId);
    if (readLastPresetId() === selectedPresetId) {
      if (nextSelectedPresetId) {
        writeLastPresetId(nextSelectedPresetId);
      } else {
        localStorage.removeItem(USER_LAST_PRESET_STORAGE_KEY);
      }
    }
    writeUserPresets(nextPresets);
    setUiNotice("Preset supprimé.");
  };

  const startSelectedRoute = async () => {
    if (graphRoutes.length === 0) {
      window.alert("Crée au moins un lien valide: entrée micro ou Son système vers une sortie audio.");
      return;
    }

    const started = await startAudioGraphRoute(graphRoutes.map((route) => route.route));
    if (started) {
      setActiveGraphRoutes(toActiveGraphRoutes(graphRoutes));
      setActiveGraphSignature(graphSignature);
    }
  };

  useEffect(() => {
    const presetUnlistenPromise = listen<string>("tray-preset-selected", (event) => {
      const preset = presetsRef.current.find((item) => item.id === event.payload);
      if (!preset) {
        setUiNotice("Preset demandé depuis la barre d'outils introuvable.");
        return;
      }
      pendingTrayPresetStartRef.current = preset.id;
      setSelectedPresetId(preset.id);
      replaceGraph(preset.nodes, preset.edges);
      writeLastPresetId(preset.id);
      setUiNotice(`Preset chargé depuis la barre d'outils: ${preset.name}`);
    });

    const audioUnlistenPromise = listen("tray-audio-toggle", () => {
      if (directRouteActiveRef.current) {
        setActiveGraphRoutes([]);
        setActiveGraphSignature("");
        void stopDirectRoute();
      } else {
        const currentRoutes = graphRoutesRef.current;
        if (currentRoutes.length === 0) {
          setUiNotice("Aucun routage valide à démarrer depuis la barre d'outils.");
          return;
        }
        void startAudioGraphRoute(currentRoutes.map((route) => route.route)).then((started) => {
          if (started) {
            setActiveGraphRoutes(toActiveGraphRoutes(currentRoutes));
            setActiveGraphSignature(graphSignatureRef.current);
          }
        });
      }
    });

    return () => {
      void presetUnlistenPromise.then((unlisten) => unlisten());
      void audioUnlistenPromise.then((unlisten) => unlisten());
    };
  }, [replaceGraph, startAudioGraphRoute, stopDirectRoute]);

  useEffect(() => {
    const pendingPresetId = pendingTrayPresetStartRef.current;
    if (!pendingPresetId || selectedPresetId !== pendingPresetId || !driverReady || graphRoutes.length === 0) {
      return;
    }

    pendingTrayPresetStartRef.current = null;
    void startAudioGraphRoute(graphRoutes.map((route) => route.route)).then((started) => {
      if (started) {
        setActiveGraphRoutes(toActiveGraphRoutes(graphRoutes));
        setActiveGraphSignature(graphSignature);
      }
    });
  }, [driverReady, graphRoutes, graphSignature, selectedPresetId, startAudioGraphRoute]);

  useEffect(() => {
    if (
      !autoStartAudio ||
      autoStartAttemptedRef.current ||
      !driverReady ||
      !defaultRoutingApplied ||
      directRouteActive ||
      graphRoutes.length === 0
    ) {
      return;
    }

    autoStartAttemptedRef.current = true;
    setStartupProgressVisible(true);
    setStartupProgress(88);
    setStartupMessage("Démarrage automatique de l'audio...");
    void startAudioGraphRoute(graphRoutes.map((route) => route.route)).then((started) => {
      if (started) {
        setActiveGraphRoutes(toActiveGraphRoutes(graphRoutes));
        setActiveGraphSignature(graphSignature);
        setStartupProgress(100);
        setStartupMessage("Audio démarré.");
        window.setTimeout(() => setStartupProgressVisible(false), 1200);
      } else {
        setStartupProgress(100);
        setStartupMessage("Démarrage automatique audio impossible.");
      }
    });
  }, [
    autoStartAudio,
    defaultRoutingApplied,
    directRouteActive,
    driverReady,
    graphRoutes,
    graphSignature,
    startAudioGraphRoute,
  ]);

  const toggleDynamicLatency = async () => {
    const nextEnabled = !dynamicLatencyEnabled;
    await setDynamicLatencyEnabled(nextEnabled);

    if (directRouteActive && graphRoutes.length > 0) {
      const started = await startAudioGraphRoute(graphRoutes.map((route) => route.route));
      if (started) {
        setActiveGraphRoutes(toActiveGraphRoutes(graphRoutes));
        setActiveGraphSignature(graphSignature);
      }
    }
  };

  const commitManualLatency = async (latencyMs: number) => {
    if (latencyMs === manualLatencyMs) {
      return;
    }

    await setManualLatencyMs(latencyMs);

    if (!dynamicLatencyEnabled && directRouteActive && graphRoutes.length > 0) {
      const started = await startAudioGraphRoute(graphRoutes.map((route) => route.route));
      if (started) {
        setActiveGraphRoutes(toActiveGraphRoutes(graphRoutes));
        setActiveGraphSignature(graphSignature);
      }
    }
  };

  return (
    <main className="app-shell">
      <header className="topbar">
        <div className="brand">
          <Cable size={16} />
          <span>VirtualAudioMix</span>
        </div>

        <section className="toolbar-group transport" aria-label="Transport audio">
          <button
            className={directRouteActive ? "transport-button active" : "transport-button"}
            onClick={() => void startSelectedRoute()}
            title="Démarrer le routage audio sélectionné"
          >
            <Play size={15} />
            <span>Démarrer audio</span>
          </button>
          <button
            className="icon-button muted"
            onClick={() => void stopSelectedRoute()}
            title="Arrêter le routage audio"
          >
            <Square size={14} />
          </button>
          <button
            className={dynamicLatencyEnabled ? "latency-toggle active" : "latency-toggle"}
            onClick={() => void toggleDynamicLatency()}
            title={
              dynamicLatencyEnabled
                ? "Latence automatique activée. Cliquer pour repasser en buffer fixe."
                : "Latence automatique désactivée. Cliquer pour l'activer."
            }
          >
            <SlidersHorizontal size={14} />
            <span>{dynamicLatencyEnabled ? "Latence auto" : "Latence fixe"}</span>
          </button>
          {!dynamicLatencyEnabled ? (
            <label className="manual-latency-control" title="Latence fixe cible appliquée au routage audio">
              <span>{pendingManualLatencyMs} ms</span>
              <input
                aria-label="Latence fixe en millisecondes"
                type="range"
                min="10"
                max="200"
                step="5"
                value={pendingManualLatencyMs}
                onChange={(event) => {
                  setPendingManualLatencyMs(Number(event.target.value));
                }}
                onPointerUp={(event) => {
                  void commitManualLatency(Number(event.currentTarget.value));
                }}
                onBlur={(event) => {
                  void commitManualLatency(Number(event.currentTarget.value));
                }}
                onKeyUp={(event) => {
                  if (["ArrowLeft", "ArrowRight", "Home", "End", "PageUp", "PageDown"].includes(event.key)) {
                    void commitManualLatency(Number(event.currentTarget.value));
                  }
                }}
              />
            </label>
          ) : null}
        </section>

        <section className="toolbar-group devices" aria-label="Périphériques audio">
          <button
            className="icon-button muted"
            onClick={() => {
              void refreshDevices();
              void refreshAudioSessions();
              void refreshVirtualDriver();
            }}
            title="Rafraîchir"
          >
            <RefreshCw size={16} />
          </button>
          <button
            className="icon-button muted"
            onClick={openAudioDefaultsSettings}
            title="Modifier le micro et la sortie par défaut"
          >
            <Settings size={16} />
          </button>
          <select
            className="device-select input-select"
            aria-label="Entrées audio"
            value=""
            onChange={(event) => {
              const device = inputDevices.find((item) => item.id === event.target.value);
              if (device) {
                addNodeInView("inputDevice", device.name, {
                  channels: device.channels,
                  channelNames: device.channelNames,
                });
              }
            }}
          >
            <option value="">Entrées</option>
            {inputDevices.map((device) => (
              <option key={device.id} value={device.id}>
                {device.name}
              </option>
            ))}
          </select>
          <select
            className="device-select output-select"
            aria-label="Sorties audio"
            value=""
            onChange={(event) => {
              const device = outputDevices.find((item) => item.id === event.target.value);
              if (device) {
                addNodeInView("outputDevice", device.name, {
                  channels: device.channels,
                  channelNames: device.channelNames,
                });
              }
            }}
          >
            <option value="">Sorties</option>
            {outputDevices.map((device) => (
              <option key={device.id} value={device.id}>
                {device.name}
              </option>
            ))}
          </select>
          <select
            className="device-select process-select"
            aria-label="Processus audio"
            value=""
            onChange={(event) => {
              const session = sessions.find((item) => item.id === event.target.value);
              if (session) {
                addNodeInView("application", session.label, {
                  channels: 2,
                  channelNames: ["L", "R"],
                  processId: session.processId ?? null,
                });
              }
            }}
          >
            <option value="">Processus</option>
            {sessions.map((session) => (
              <option key={session.id} value={session.id}>
                {session.label}
              </option>
            ))}
          </select>
        </section>

        <section className="toolbar-group presets" aria-label="Presets utilisateur">
          <select
            className="device-select preset-select"
            aria-label="Presets"
            value={selectedPresetId}
            onChange={(event) => setSelectedPresetId(event.target.value)}
          >
            {presets.length === 0 ? <option value="">Presets</option> : null}
            {presets.map((preset) => (
              <option key={preset.id} value={preset.id}>
                {preset.name}
              </option>
            ))}
          </select>
          <button className="icon-button muted" onClick={loadSelectedPreset} title="Charger le preset sélectionné">
            <Play size={14} />
          </button>
          <button className="icon-button muted" onClick={saveCurrentPreset} title="Sauvegarder le graphe courant">
            <Plus size={15} />
          </button>
          <div className="preset-export-group">
            <button
              className="icon-button muted"
              aria-expanded={presetExportMenuOpen}
              aria-haspopup="menu"
              onClick={() => setPresetExportMenuOpen((open) => !open)}
              title="Exporter des presets en JSON"
            >
              <Download size={14} />
            </button>
            {presetExportMenuOpen ? (
              <div className="preset-export-menu" role="menu">
                <button type="button" role="menuitem" onClick={exportSelectedPreset}>
                  Ce preset
                </button>
                <button type="button" role="menuitem" onClick={exportAllPresets}>
                  Tous les presets
                </button>
              </div>
            ) : null}
          </div>
          <button className="icon-button muted" onClick={() => presetImportInputRef.current?.click()} title="Importer un ou plusieurs presets JSON">
            <Upload size={14} />
          </button>
          <input
            ref={presetImportInputRef}
            className="visually-hidden"
            accept="application/json,.json,.vam-preset.json"
            multiple
            type="file"
            onChange={importPresetFile}
          />
          <button className="icon-button muted" onClick={deleteSelectedPreset} title="Supprimer le preset sélectionné">
            <XCircle size={14} />
          </button>
        </section>

        <section className="toolbar-group driver-status" aria-label="Driver virtuel">
          <span className={driverReady ? "status-pill ok" : "status-pill error"}>
            {driverReady ? <CheckCircle2 size={14} /> : <XCircle size={14} />}
            <span>{driverReady ? "Driver BAD OK" : "Driver BAD absent"}</span>
          </span>
          <span className="status-detail driver-detail">
            {virtualDriver
              ? `service ${virtualDriver.serviceRunning ? "actif" : "inactif"} · entrée ${
                  virtualDriver.inputEndpointOk ? "OK" : "KO"
                } · sortie ${virtualDriver.outputEndpointOk ? "OK" : "KO"}`
              : "diagnostic non chargé"}
          </span>
        </section>
      </header>

      {startupProgressVisible ? (
        <div className="startup-progress-banner" role="status" aria-live="polite">
          <div className="startup-progress-copy">
            <span>{startupMessage}</span>
            <strong>{Math.round(startupProgress)}%</strong>
          </div>
          <div className="startup-progress-track">
            <span style={{ inlineSize: `${startupProgress}%` }} />
          </div>
        </div>
      ) : null}

      {audioError ? <div className="error-banner">{audioError}</div> : null}
      {!audioError && (audioStatus || transportStatus || uiNotice || missingDeviceLabels.length > 0) ? (
        <div className="info-banner status-row">
          {uiNotice ? (
            <span className="ui-notice">{uiNotice}</span>
          ) : missingDeviceLabels.length > 0 ? (
            <span>Périphérique absent: {missingDeviceLabels.join(", ")}</span>
          ) : audioStatus ? (
            <span>{audioStatus}</span>
          ) : (
            <span>Audio prêt.</span>
          )}
          {transportStatus ? (
            <span className="runtime-diagnostics" title={formatTransportMetrics(transportStatus, runtimeMetrics, "full")}>
              {formatTransportMetrics(transportStatus, runtimeMetrics)}
            </span>
          ) : null}
          {transportStatus ? (
            <button className="details-button" onClick={() => setDetailsDialogOpen(true)}>
              Détails
            </button>
          ) : null}
        </div>
      ) : null}
      <GraphCanvas />
      {detailsDialogOpen ? (
        <aside className="diagnostic-panel" aria-label="Détails audio">
          <header>
            <div>
              <h2>Détails audio</h2>
              <p>Diagnostic en lecture seule du transport et du moteur Rust.</p>
            </div>
            <button className="icon-button muted" onClick={() => setDetailsDialogOpen(false)} title="Fermer">
              <XCircle size={15} />
            </button>
          </header>
          <DiagnosticDetails
            transportStatus={transportStatus}
            runtimeMetrics={runtimeMetrics}
            driverReady={Boolean(driverReady)}
            directRouteActive={directRouteActive}
            graphRoutesCount={graphRoutes.length}
            history={diagnosticHistory}
          />
        </aside>
      ) : null}
      {defaultsDialogOpen ? (
        <div className="modal-backdrop" role="presentation">
          <form className="setup-dialog" onSubmit={saveUserDefaults}>
            <h2>Configuration audio par défaut</h2>
            <p>
              Choisis ton micro et ta sortie d'écoute. VAM créera ensuite automatiquement le graphe de départ à
              chaque lancement.
            </p>

            <label>
              Micro par défaut
              <select value={selectedDefaultInput} onChange={(event) => setSelectedDefaultInput(event.target.value)}>
                {selectableDefaultInputs.map((device) => (
                  <option key={device.id} value={device.name}>
                    {device.name}
                  </option>
                ))}
              </select>
            </label>

            <label>
              Enceintes / casque par défaut
              <select value={selectedDefaultOutput} onChange={(event) => setSelectedDefaultOutput(event.target.value)}>
                {selectableDefaultOutputs.map((device) => (
                  <option key={device.id} value={device.name}>
                    {device.name}
                  </option>
                ))}
              </select>
            </label>

            <section className="settings-section">
              <h3>Démarrage</h3>
              <label className="settings-toggle">
                <input
                  type="checkbox"
                  checked={startWithWindowsEnabled}
                  disabled={startWithWindowsBusy}
                  onChange={(event) => void updateStartWithWindows(event.target.checked)}
                />
                <span>Démarrer VirtualAudioMix avec Windows</span>
              </label>
              <label className="settings-toggle">
                <input
                  type="checkbox"
                  checked={autoStartAudio}
                  onChange={(event) => updateAutoStartAudio(event.target.checked)}
                />
                <span>Démarrer automatiquement le moteur audio après chargement du driver BAD</span>
              </label>
            </section>

            <section className="settings-section">
              <h3>Presets JSON</h3>
              <p>
                Dossier d'export actuel: <strong>{presetExportDirectory ? presetExportDirectoryName : "Téléchargements"}</strong>.
                {presetExportDirectory ? "" : " Sans dossier choisi, VAM utilise le téléchargement standard."}
              </p>
              <button className="transport-button" type="button" onClick={choosePresetExportDirectory}>
                Choisir le dossier d'export
              </button>
            </section>

            <section className="settings-section">
              <h3>À propos</h3>
              <p>Informations sur le projet, l'auteur et l'esprit de VirtualAudioMix.</p>
              <button className="transport-button" type="button" onClick={() => setAboutDialogOpen(true)}>
                <Info size={14} />
                À propos de VAM
              </button>
            </section>

            <div className="setup-dialog-actions">
              {defaultRoutingApplied ? (
                <button className="transport-button" type="button" onClick={() => setDefaultsDialogOpen(false)}>
                  Annuler
                </button>
              ) : null}
              <button className="transport-button active" disabled={!selectedDefaultInput || !selectedDefaultOutput}>
                Enregistrer
              </button>
            </div>
          </form>
        </div>
      ) : null}
      {aboutDialogOpen ? (
        <div className="modal-backdrop" role="presentation">
          <section className="setup-dialog about-dialog" role="dialog" aria-modal="true" aria-label="À propos de VirtualAudioMix">
            <h2>À propos de VirtualAudioMix</h2>
            <p>
              VirtualAudioMix est un logiciel gratuit, sans inscription, sans tracker et sans besoin d'internet pour
              fonctionner.
            </p>
            <p>
              Il est développé en Rust et React par Bruno Del piero. Le développement n'est pas mon métier; si vous
              avez le moindre souci, contactez-moi.
            </p>
            <p>
              Si vous voulez contribuer, modifier ou améliorer quoi que ce soit, faites-vous plaisir. Des bisous.
            </p>
            <p>
              GitHub:{" "}
              <a href="https://github.com/8run0d/VirtualAudioMix" target="_blank" rel="noreferrer">
                https://github.com/8run0d/VirtualAudioMix
              </a>
            </p>
            <div className="setup-dialog-actions">
              <button className="transport-button active" type="button" onClick={() => setAboutDialogOpen(false)}>
                Fermer
              </button>
            </div>
          </section>
        </div>
      ) : null}
    </main>
  );
}

function readUserAudioDefaults(): UserAudioDefaults | null {
  try {
    const raw = localStorage.getItem(USER_AUDIO_DEFAULTS_STORAGE_KEY);
    if (!raw) {
      return null;
    }
    const parsed = JSON.parse(raw) as Partial<UserAudioDefaults>;
    if (typeof parsed.inputDeviceName !== "string" || typeof parsed.outputDeviceName !== "string") {
      return null;
    }
    return {
      inputDeviceName: parsed.inputDeviceName,
      outputDeviceName: parsed.outputDeviceName,
    };
  } catch {
    return null;
  }
}

function readUserAppPreferences(): UserAppPreferences {
  try {
    const raw = localStorage.getItem(USER_APP_PREFERENCES_STORAGE_KEY);
    if (!raw) {
      return { autoStartAudio: true };
    }
    const parsed = JSON.parse(raw) as Partial<UserAppPreferences>;
    return { autoStartAudio: parsed.autoStartAudio !== false };
  } catch {
    return { autoStartAudio: true };
  }
}

function writeUserAppPreferences(preferences: UserAppPreferences) {
  localStorage.setItem(USER_APP_PREFERENCES_STORAGE_KEY, JSON.stringify(preferences));
}

function readUserPresets(): UserPreset[] {
  try {
    const raw = localStorage.getItem(USER_PRESETS_STORAGE_KEY);
    if (!raw) {
      return [];
    }
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) {
      return [];
    }
    return parsed
      .map((preset) => normalizeUserPreset(preset))
      .filter((preset): preset is UserPreset => preset !== null);
  } catch {
    return [];
  }
}

function writeUserPresets(presets: UserPreset[]) {
  localStorage.setItem(USER_PRESETS_STORAGE_KEY, JSON.stringify(presets));
}

function readLastPresetId() {
  return localStorage.getItem(USER_LAST_PRESET_STORAGE_KEY) ?? "";
}

function writeLastPresetId(presetId: string) {
  localStorage.setItem(USER_LAST_PRESET_STORAGE_KEY, presetId);
}

function isUserPreset(value: unknown): value is UserPreset {
  if (!value || typeof value !== "object") {
    return false;
  }

  const preset = value as Partial<UserPreset>;
  return (
    typeof preset.id === "string" &&
    typeof preset.name === "string" &&
    typeof preset.createdAt === "string" &&
    Array.isArray(preset.nodes) &&
    Array.isArray(preset.edges)
  );
}

function normalizeUserPreset(value: unknown, fallbackName = "Preset importé"): UserPreset | null {
  if (!value || typeof value !== "object") {
    return null;
  }

  const preset = value as Partial<UserPreset>;
  if (!Array.isArray(preset.nodes) || !Array.isArray(preset.edges)) {
    return null;
  }

  return {
    id: typeof preset.id === "string" && preset.id.trim() ? preset.id : crypto.randomUUID(),
    name: typeof preset.name === "string" && preset.name.trim() ? preset.name.trim() : fallbackName,
    createdAt: typeof preset.createdAt === "string" ? preset.createdAt : new Date().toISOString(),
    nodes: serializePresetNodes(preset.nodes as AudioFlowNode[]),
    edges: serializePresetEdges(preset.edges as AudioFlowEdge[]),
  };
}

function normalizePresetImport(value: unknown, fallbackName: string): UserPreset[] {
  if (value && typeof value === "object" && Array.isArray((value as Partial<PresetBundle>).presets)) {
    return ((value as Partial<PresetBundle>).presets ?? [])
      .map((preset) => normalizeUserPreset(preset, fallbackName))
      .filter((preset): preset is UserPreset => preset !== null);
  }

  const preset = normalizeUserPreset(value, fallbackName.replace(/\.vam-preset\.json$|\.json$/i, ""));
  return preset ? [preset] : [];
}

async function writeJsonToDirectory(directory: WritableDirectoryHandle, filename: string, content: string) {
  const file = await directory.getFileHandle(filename, { create: true });
  const writable = await file.createWritable();
  await writable.write(new Blob([content], { type: "application/json" }));
  await writable.close();
}

function downloadJsonFile(filename: string, content: string) {
  const blob = new Blob([content], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  URL.revokeObjectURL(url);
}

function getDirectoryPicker() {
  const maybeWindow = window as typeof window & {
    showDirectoryPicker?: (options: { mode: "read" | "readwrite" }) => Promise<WritableDirectoryHandle>;
  };
  return maybeWindow.showDirectoryPicker;
}

function serializePresetNodes(nodes: AudioFlowNode[]): AudioFlowNode[] {
  return nodes.map((node) => ({
    id: node.id,
    type: "audioNode",
    position: {
      x: Number(node.position?.x ?? 0),
      y: Number(node.position?.y ?? 0),
    },
    data: {
      kind: node.data.kind,
      label: node.data.label,
      channels: sanitizePresetChannelCount(node.data.channels),
      channelNames: normalizePresetChannelNames(
        node.data.channelNames,
        sanitizePresetChannelCount(node.data.channels),
        node.data.kind,
      ),
      expanded: node.data.expanded === true,
      enabled: node.data.enabled !== false,
      level: 0,
      bands: [0, 0, 0, 0, 0, 0, 0, 0],
      waveform: Array.from({ length: 48 }, () => 0),
      visualGain: 1,
      processId: Number.isFinite(Number(node.data.processId)) ? Number(node.data.processId) : null,
    },
  }));
}

function serializePresetEdges(edges: AudioFlowEdge[]): AudioFlowEdge[] {
  return edges.map((edge) => {
    const sourceChannel = normalizePresetChannel(edge.data?.sourceChannel);
    const targetChannel = normalizePresetChannel(edge.data?.targetChannel);
    return {
      id: edge.id,
      source: edge.source,
      target: edge.target,
      sourceHandle: normalizePresetHandle(edge.sourceHandle, sourceChannel),
      targetHandle: normalizePresetHandle(edge.targetHandle, targetChannel),
      type: "audioEdge",
      data: {
        gain: sanitizePresetGain(edge.data?.gain),
        level: 0,
        sourceChannel,
        targetChannel,
      },
    };
  });
}

function sanitizePresetGain(value: unknown) {
  const gain = Number(value ?? 1);
  return Number.isFinite(gain) ? Math.min(4, Math.max(0, gain)) : 1;
}

function sanitizePresetChannelCount(value: unknown) {
  const channels = Number(value ?? 1);
  return Number.isFinite(channels) ? Math.min(16, Math.max(1, Math.floor(channels))) : 1;
}

function normalizePresetChannel(value: unknown): number | "all" {
  if (value === "all") {
    return "all";
  }
  const channel = Number(value);
  return Number.isInteger(channel) && channel >= 0 && channel < 16 ? channel : "all";
}

function normalizePresetHandle(handle: string | null | undefined, channel: number | "all") {
  if (handle === "all" || handle?.startsWith("ch-")) {
    return handle;
  }
  return channel === "all" ? "all" : `ch-${channel}`;
}

function normalizePresetChannelNames(value: unknown, channels: number, kind: unknown) {
  const names = Array.isArray(value) ? value.map((name) => String(name)) : [];
  return Array.from(
    { length: channels },
    (_, index) =>
      names[index] ||
      (channels === 2 && kind !== "inputDevice" ? ["L", "R"][index] : `Canal ${index + 1}`),
  );
}

function safeFilename(value: string) {
  return value.trim().replace(/[<>:"/\\|?*\u0000-\u001F]/g, "_") || "preset";
}

function buildGraphRoutes(
  nodes: AudioFlowNode[],
  edges: AudioFlowEdge[],
  devices: AudioDevice[],
  sessions: AudioSession[],
): GraphRouteDescriptor[] {
  const availableInputs = new Set(devices.filter((device) => device.kind === "input").map((device) => device.name));
  const availableOutputs = new Set(devices.filter((device) => device.kind === "output").map((device) => device.name));
  const availableApplications = new Set(sessions.map((session) => session.label));
  return edges
    .map<GraphRouteDescriptor | null>((edge) => {
      const source = nodes.find((node) => node.id === edge.source);
      const target = nodes.find((node) => node.id === edge.target);

      if (!source || !target) {
        return null;
      }

      if (source.data.enabled === false || target.data.enabled === false) {
        return null;
      }

      const sourceKind =
        source.data.kind === "inputDevice" ||
        source.data.kind === "systemAudio" ||
        source.data.kind === "virtualOutput" ||
        source.data.kind === "application"
          ? source.data.kind
          : null;
      const targetKind: AudioGraphRoute["targetKind"] | null =
        target.data.kind === "outputDevice"
          ? "outputDevice"
          : target.data.kind === "inputDevice" && isVirtualInputNode(target.data.label)
            ? "virtualInput"
            : null;

      if (!sourceKind || !targetKind) {
        return null;
      }

      const sourceProcessId = sourceKind === "application" ? Number(source.data.processId) : undefined;
      if (sourceKind === "application" && (!Number.isFinite(sourceProcessId) || !sourceProcessId)) {
        return null;
      }

      if (!isSourceAvailable(source, sourceKind, availableInputs, availableOutputs, availableApplications)) {
        return null;
      }

      if (!isTargetAvailable(target, targetKind, availableInputs, availableOutputs)) {
        return null;
      }

      const descriptor: GraphRouteDescriptor = {
        edgeId: edge.id,
        sourceNodeId: source.id,
        targetNodeId: target.id,
        route: {
            sourceKind: sourceKind === "virtualOutput" ? "systemAudio" : sourceKind,
            sourceNodeId: source.id,
            sourceName: sourceKind === "inputDevice" ? source.data.label : undefined,
            sourceProcessId,
            sourceChannel: edge.data?.sourceChannel ?? "all",
            targetKind,
            targetNodeId: target.id,
            targetName: target.data.label,
            targetChannel: edge.data?.targetChannel ?? "all",
            gain: Number(edge.data?.gain ?? 1),
        },
      };
      return descriptor;
    })
    .filter((route): route is GraphRouteDescriptor => route !== null);
}

function isSourceAvailable(
  source: AudioFlowNode,
  sourceKind: AudioNodeKind,
  availableInputs: Set<string>,
  availableOutputs: Set<string>,
  availableApplications: Set<string>,
) {
  if (sourceKind === "inputDevice") {
    return availableInputs.has(source.data.label);
  }
  if (sourceKind === "virtualOutput") {
    return availableOutputs.has(source.data.label);
  }
  if (sourceKind === "application") {
    return availableApplications.has(source.data.label);
  }
  return true;
}

function isTargetAvailable(
  target: AudioFlowNode,
  targetKind: AudioGraphRoute["targetKind"],
  availableInputs: Set<string>,
  availableOutputs: Set<string>,
) {
  if (targetKind === "virtualInput") {
    return availableInputs.has(target.data.label);
  }
  return availableOutputs.has(target.data.label);
}

function findMissingDeviceLabels(nodes: AudioFlowNode[], devices: AudioDevice[], sessions: AudioSession[]) {
  const availableDeviceNames = new Set([...devices.map((device) => device.name), ...sessions.map((session) => session.label)]);
  const missing = new Set<string>();
  for (const node of nodes) {
    const requiresDevice =
      node.data.kind === "inputDevice" ||
      node.data.kind === "outputDevice" ||
      node.data.kind === "virtualOutput" ||
      node.data.kind === "application";
    if (requiresDevice && !availableDeviceNames.has(node.data.label)) {
      missing.add(node.data.label);
    }
  }
  return Array.from(missing);
}

function toActiveGraphRoutes(graphRoutes: GraphRouteDescriptor[]) {
  return graphRoutes.map((route) => ({
    edgeId: route.edgeId,
    sourceNodeId: route.sourceNodeId,
    targetNodeId: route.targetNodeId,
  }));
}

function createGraphRouteSignature(graphRoutes: GraphRouteDescriptor[]) {
  return JSON.stringify(
    graphRoutes.map((route) => ({
      edgeId: route.edgeId,
      sourceNodeId: route.sourceNodeId,
      targetNodeId: route.targetNodeId,
      ...route.route,
    })),
  );
}

function createGraphDefinitionSignature(nodes: AudioFlowNode[], edges: AudioFlowEdge[]) {
  return JSON.stringify({
    nodes: nodes.map((node) => ({
      id: node.id,
      kind: node.data.kind,
      label: node.data.label,
      enabled: node.data.enabled !== false,
    })),
    edges: edges.map((edge) => ({
      id: edge.id,
      source: edge.source,
      target: edge.target,
      gain: Number(edge.data?.gain ?? 1),
      sourceChannel: edge.data?.sourceChannel ?? "all",
      targetChannel: edge.data?.targetChannel ?? "all",
    })),
  });
}

function DiagnosticDetails({
  transportStatus,
  runtimeMetrics,
  driverReady,
  directRouteActive,
  graphRoutesCount,
  history,
}: {
  transportStatus: AudioTransportStatus | null;
  runtimeMetrics: AudioRuntimeMetrics | null;
  driverReady: boolean;
  directRouteActive: boolean;
  graphRoutesCount: number;
  history: DiagnosticHistoryEntry[];
}) {
  if (!transportStatus) {
    return <p className="diagnostic-empty">Aucun statut transport disponible.</p>;
  }

  const inputMs = bytesToMs(
    transportStatus.captureMaxReaderAvailableBytes,
    transportStatus.sampleRate,
    transportStatus.channels,
    transportStatus.bitsPerSample,
  );
  const outputMs = bytesToMs(
    transportStatus.renderAvailableBytes,
    transportStatus.sampleRate,
    transportStatus.channels,
    transportStatus.bitsPerSample,
  );
  const latencyMode = runtimeMetrics?.dynamicLatencyEnabled === false ? "Fixe" : "Auto";
  const latencyTarget =
    runtimeMetrics?.dynamicLatencyEnabled === false
      ? `${runtimeMetrics.manualLatencyTargetMs}/${runtimeMetrics.manualLatencyTargetMs * 4} ms`
      : runtimeMetrics && runtimeMetrics.dynamicLatencyTargetMs > 0
      ? `${runtimeMetrics.dynamicLatencyTargetMs}/${runtimeMetrics.dynamicLatencyMaxMs} ms`
      : "non mesuré";
  const writerLate =
    runtimeMetrics && runtimeMetrics.vamCaptureWriterLateSamples > 0
      ? `${usToMs(runtimeMetrics.vamCaptureWriterLateAvgUs)} ms moyen / ${usToMs(
          runtimeMetrics.vamCaptureWriterLateMaxUs,
        )} ms max`
      : "aucun retard mesuré";
  const firstHistory = history[0];
  const lastHistory = history.length > 0 ? history[history.length - 1] : undefined;
  const historyDurationSeconds =
    firstHistory && lastHistory ? Math.max(0, Math.round((lastHistory.at - firstHistory.at) / 1000)) : 0;
  const captureUnderrunDelta =
    firstHistory && lastHistory ? lastHistory.captureUnderrunBytes - firstHistory.captureUnderrunBytes : 0;
  const captureOverflowDelta =
    firstHistory && lastHistory ? lastHistory.captureOverflowBytes - firstHistory.captureOverflowBytes : 0;
  const renderOverflowDelta =
    firstHistory && lastHistory ? lastHistory.renderOverflowBytes - firstHistory.renderOverflowBytes : 0;
  const maxWriterLateUs = history.reduce((max, entry) => Math.max(max, entry.writerLateMaxUs), 0);

  return (
    <div className="diagnostic-grid">
      <section>
        <h3>État</h3>
        <DiagnosticRow label="Driver BAD" value={driverReady ? "OK" : "KO"} />
        <DiagnosticRow label="Routage actif" value={directRouteActive ? "oui" : "non"} />
        <DiagnosticRow label="Liens audio valides" value={String(graphRoutesCount)} />
        <DiagnosticRow label="Mode latence" value={latencyMode} />
        <DiagnosticRow label="Cible / max" value={latencyTarget} />
        <DiagnosticRow
          label="Analyse FFT"
          value={
            runtimeMetrics
              ? `${runtimeMetrics.audioVisualizerFftEnabled ? "active" : "fallback"} · ${runtimeMetrics.audioVisualizerFftLastUs} µs`
              : "non chargée"
          }
        />
      </section>

      <section>
        <h3>Transport</h3>
        <DiagnosticRow label="Format" value={`${transportStatus.sampleRate} Hz · ${transportStatus.channels} ch · ${transportStatus.bitsPerSample} bit`} />
        <DiagnosticRow label="Capture disponible" value={`${inputMs} ms`} />
        <DiagnosticRow label="Render disponible" value={`${outputMs} ms`} />
        <DiagnosticRow label="Clients capture" value={String(transportStatus.captureActiveReaders)} />
        <DiagnosticRow label="Writer Rust" value={writerLate} />
      </section>

      <section>
        <h3>Compteurs</h3>
        <DiagnosticRow label="Underruns capture" value={String(transportStatus.captureUnderrunBytes)} />
        <DiagnosticRow label="Overflows capture" value={String(transportStatus.captureOverflowBytes)} />
        <DiagnosticRow label="Overflows render" value={String(transportStatus.renderOverflowBytes)} />
        <DiagnosticRow label="Ajustements auto" value={String(runtimeMetrics?.dynamicLatencyAdjustments ?? 0)} />
        <DiagnosticRow label="Overflows adaptation" value={String(runtimeMetrics?.dynamicLatencyOverflowEvents ?? 0)} />
      </section>

      <section>
        <h3>Historique léger</h3>
        <DiagnosticRow label="Fenêtre" value={historyDurationSeconds > 0 ? `${historyDurationSeconds} s` : "en attente"} />
        <DiagnosticRow label="Δ underruns capture" value={String(captureUnderrunDelta)} />
        <DiagnosticRow label="Δ overflows capture" value={String(captureOverflowDelta)} />
        <DiagnosticRow label="Δ overflows render" value={String(renderOverflowDelta)} />
        <DiagnosticRow label="Max writer récent" value={maxWriterLateUs > 0 ? `${usToMs(maxWriterLateUs)} ms` : "0 ms"} />
        <div className="diagnostic-sparkline" aria-label="Historique visuel du buffer capture">
          {history.slice(-30).map((entry) => (
            <span
              key={entry.at}
              title={`capture ${entry.captureMs} ms · render ${entry.renderMs} ms`}
              style={{ blockSize: `${Math.max(4, Math.min(34, entry.captureMs))}px` }}
            />
          ))}
        </div>
      </section>
    </div>
  );
}

function DiagnosticRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="diagnostic-row">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function formatTransportMetrics(
  status: AudioTransportStatus,
  runtimeMetrics: AudioRuntimeMetrics | null,
  mode: "short" | "full" = "short",
) {
  const captureMs = bytesToMs(
    status.captureMaxReaderAvailableBytes,
    status.sampleRate,
    status.channels,
    status.bitsPerSample,
  );
  const renderMs = bytesToMs(status.renderAvailableBytes, status.sampleRate, status.channels, status.bitsPerSample);
  const writerLate =
    runtimeMetrics && runtimeMetrics.vamCaptureWriterLateSamples > 0
      ? `${usToMs(runtimeMetrics.vamCaptureWriterLateAvgUs)}/${usToMs(runtimeMetrics.vamCaptureWriterLateMaxUs)} ms`
      : "--/-- ms";
  const autoLatency =
    runtimeMetrics?.dynamicLatencyEnabled === false
      ? `${runtimeMetrics.manualLatencyTargetMs}/${runtimeMetrics.manualLatencyTargetMs * 4} ms`
      : runtimeMetrics && runtimeMetrics.dynamicLatencyTargetMs > 0
      ? `${runtimeMetrics.dynamicLatencyTargetMs}/${runtimeMetrics.dynamicLatencyMaxMs} ms`
      : "--/-- ms";
  const latencyMode = runtimeMetrics?.dynamicLatencyEnabled === false ? "fixe" : "auto";
  const adjustments =
    mode === "full" && runtimeMetrics
      ? ` · adj ${runtimeMetrics.dynamicLatencyAdjustments} · ov ${runtimeMetrics.dynamicLatencyOverflowEvents}`
      : "";
  const fft =
    mode === "full" && runtimeMetrics
      ? ` · fft ${runtimeMetrics.audioVisualizerFftEnabled ? "on" : "fallback"} ${runtimeMetrics.audioVisualizerFftLastUs}µs fb ${runtimeMetrics.audioVisualizerFftFallbacks}`
      : "";
  const underruns = mode === "full" ? String(status.captureUnderrunBytes) : compactCount(status.captureUnderrunBytes);
  const overflows = mode === "full" ? String(status.captureOverflowBytes) : compactCount(status.captureOverflowBytes);
  return `diag · cap ${status.captureActiveReaders} client${status.captureActiveReaders > 1 ? "s" : ""} · in ${captureMs} ms · out ${renderMs} ms · xr ${underruns}/${overflows} · wr ${writerLate} · ${latencyMode} ${autoLatency}${adjustments}${fft}`;
}

function usToMs(valueUs: number) {
  return (valueUs / 1_000).toFixed(valueUs >= 10_000 ? 0 : 1);
}

function compactCount(value: number) {
  if (value >= 1_000_000_000) {
    return `${(value / 1_000_000_000).toFixed(1)}G`;
  }
  if (value >= 1_000_000) {
    return `${(value / 1_000_000).toFixed(1)}M`;
  }
  if (value >= 1_000) {
    return `${(value / 1_000).toFixed(1)}k`;
  }
  return String(value);
}

function getVisibleNodePosition(
  nodes: AudioFlowNode[],
  screenToFlowPosition: (position: XYPosition) => XYPosition,
  preferredPosition?: XYPosition,
) {
  const canvas = document.querySelector<HTMLElement>(".canvas");
  if (!canvas) {
    return preferredPosition;
  }

  const rect = canvas.getBoundingClientRect();
  const topLeft = screenToFlowPosition({
    x: rect.left + CANVAS_MARGIN,
    y: rect.top + CANVAS_MARGIN,
  });
  const bottomRight = screenToFlowPosition({
    x: rect.right - CANVAS_MARGIN,
    y: rect.bottom - CANVAS_MARGIN,
  });

  const minX = Math.min(topLeft.x, bottomRight.x);
  const maxX = Math.max(topLeft.x, bottomRight.x);
  const minY = Math.min(topLeft.y, bottomRight.y);
  const maxY = Math.max(topLeft.y, bottomRight.y);
  const lastVisibleX = Math.max(minX, maxX - ESTIMATED_NODE_WIDTH);
  const lastVisibleY = Math.max(minY, maxY - ESTIMATED_NODE_HEIGHT);

  if (preferredPosition && isNodeVisible(preferredPosition, minX, minY, maxX, maxY) && !overlapsExistingNode(preferredPosition, nodes)) {
    return roundPosition(preferredPosition);
  }

  for (let y = minY; y <= lastVisibleY; y += ESTIMATED_NODE_HEIGHT + NODE_GAP_Y) {
    for (let x = minX; x <= lastVisibleX; x += ESTIMATED_NODE_WIDTH + NODE_GAP_X) {
      const position = { x, y };
      if (!overlapsExistingNode(position, nodes)) {
        return roundPosition(position);
      }
    }
  }

  const offset = (nodes.length % 6) * 24;
  const fallback = {
    x: clamp(minX + offset, minX, lastVisibleX),
    y: clamp(minY + offset, minY, lastVisibleY),
  };
  if (!overlapsExistingNode(fallback, nodes)) {
    return roundPosition(fallback);
  }

  const occupiedBounds = getOccupiedBounds(nodes);
  return roundPosition({
    x: occupiedBounds ? occupiedBounds.maxX + NODE_GAP_X : fallback.x,
    y: occupiedBounds ? occupiedBounds.minY : fallback.y,
  });
}

function isNodeVisible(position: XYPosition, minX: number, minY: number, maxX: number, maxY: number) {
  return (
    position.x >= minX &&
    position.y >= minY &&
    position.x + ESTIMATED_NODE_WIDTH <= maxX &&
    position.y + ESTIMATED_NODE_HEIGHT <= maxY
  );
}

function overlapsExistingNode(position: XYPosition, nodes: AudioFlowNode[]) {
  return nodes.some((node) => {
    const other = node.position;
    return (
      position.x < other.x + ESTIMATED_NODE_WIDTH &&
      position.x + ESTIMATED_NODE_WIDTH > other.x &&
      position.y < other.y + ESTIMATED_NODE_HEIGHT &&
      position.y + ESTIMATED_NODE_HEIGHT > other.y
    );
  });
}

function getOccupiedBounds(nodes: AudioFlowNode[]) {
  if (nodes.length === 0) {
    return null;
  }

  return nodes.reduce(
    (bounds, node) => ({
      minX: Math.min(bounds.minX, node.position.x),
      minY: Math.min(bounds.minY, node.position.y),
      maxX: Math.max(bounds.maxX, node.position.x + ESTIMATED_NODE_WIDTH),
      maxY: Math.max(bounds.maxY, node.position.y + ESTIMATED_NODE_HEIGHT),
    }),
    {
      minX: Number.POSITIVE_INFINITY,
      minY: Number.POSITIVE_INFINITY,
      maxX: Number.NEGATIVE_INFINITY,
      maxY: Number.NEGATIVE_INFINITY,
    },
  );
}

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

function roundPosition(position: XYPosition) {
  return {
    x: Math.round(position.x),
    y: Math.round(position.y),
  };
}

function bytesToMs(bytes: number, sampleRate: number, channels: number, bitsPerSample: number) {
  const bytesPerFrame = channels * Math.max(1, bitsPerSample / 8);
  if (!sampleRate || !bytesPerFrame) {
    return 0;
  }
  return Math.round((bytes / bytesPerFrame / sampleRate) * 1_000);
}

function isVirtualInputNode(label: string) {
  return (
    label.includes("VAM Entrée") ||
    label.includes("VAM IN") ||
    label.includes("Bubux Audio Driver") ||
    label.includes("VirtualAudioMix Audio Driver")
  );
}

function isVirtualOutputNode(label: string) {
  return label.includes("VAM Sortie") || label.includes("VAM OUT") || label.includes("Bubux Audio Driver");
}
