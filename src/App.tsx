import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open, save } from "@tauri-apps/plugin-dialog";
import { readTextFile, writeTextFile } from "@tauri-apps/plugin-fs";
import { Store } from "@tauri-apps/plugin-store";

interface BridgeConfig {
  console_ip: string;
  rcp_port: number;
  udp_osc_out_addr: string;
  udp_osc_out_port: number;
  udp_osc_in_addr: string;
  udp_osc_in_port: number;
}

interface BridgeResponse {
  success: boolean;
  message: string;
}

type LogLevel = "DEBUG" | "INFO" | "WARN" | "ERROR";

interface LogEntry {
  level: LogLevel;
  message: string;
}

const LOG_LEVEL_COLORS: Record<LogLevel, string> = {
  DEBUG: "text-slate-500",
  INFO: "text-green-400",
  WARN: "text-yellow-400",
  ERROR: "text-red-400",
};

export default function App() {
  const [initializing, setInitializing] = useState(true);
  const [isRunning, setIsRunning] = useState(false);
  const [isTransitioning, setIsTransitioning] = useState(false);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [config, setConfig] = useState<BridgeConfig>({
    console_ip: "",
    rcp_port: 49280,
    udp_osc_out_addr: "127.0.0.1",
    udp_osc_out_port: 3999,
    udp_osc_in_addr: "0.0.0.0",
    udp_osc_in_port: 4000,
  });
  const [lastConfigPath, setLastConfigPath] = useState<string | null>(null);
  const [showQuitPrompt, setShowQuitPrompt] = useState(false);
  const configRef = useRef(config);
  // The config as it exists on disk/in the store, i.e. as of the last
  // load/save. Compared against configRef to detect unsaved changes on quit.
  const savedConfigRef = useRef(config);
  const logsEndRef = useRef<HTMLDivElement>(null);
  // Guards against React.StrictMode's dev-mode double-invoke running the
  // (non-idempotent) auto-reconnect start_bridge call twice, racing itself
  // into a spurious "Bridge is already running" error.
  const didInit = useRef(false);

  useEffect(() => {
    configRef.current = config;
  }, [config]);

  useEffect(() => {
    if (didInit.current) return;
    didInit.current = true;

    const initStore = async () => {
      try {
        const storeInstance = await Store.load("config.json");
        const savedPath = await storeInstance.get<string>("last_config_path");
        if (savedPath) {
          setLastConfigPath(savedPath);
        }
        const savedConfig =
          await storeInstance.get<BridgeConfig>("bridge_config");
        let effectiveConfig = configRef.current;
        if (savedConfig) {
          effectiveConfig = { ...configRef.current, ...savedConfig };
          setConfig(effectiveConfig);
          savedConfigRef.current = effectiveConfig;
        }

        const status = await invoke<boolean>("get_bridge_status");
        if (status) {
          setIsRunning(true);
        } else {
          const wasRunning = await storeInstance.get<boolean>("was_running");
          if (wasRunning) {
            // Logged before the (async) start attempt so it's guaranteed to
            // precede any bridge-log events the backend emits while
            // connecting — otherwise a fast connection failure can log
            // before this "success" confirmation, reading as contradictory.
            setLogs((prev) => [
              ...prev,
              {
                level: "INFO",
                message: "Reconnecting to previous session...",
              },
            ]);
            const res = await invoke<BridgeResponse>("start_bridge", {
              config: effectiveConfig,
            });
            if (res.success) {
              setIsRunning(true);
            } else {
              setLogs((prev) => [
                ...prev,
                { level: "ERROR", message: res.message },
              ]);
            }
          }
        }
      } catch (e) {
        console.error("Failed to initialize store", e);
      } finally {
        setInitializing(false);
      }
    };
    initStore();
  }, []);

  useEffect(() => {
    logsEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [logs]);

  // Writes the current config to `targetPath` (prompting for a path if none
  // is known yet), and updates the store + saved-snapshot ref. Returns
  // whether the save succeeded, so callers can decide whether it's safe to
  // proceed (e.g. quitting).
  const saveConfig = useCallback(async (): Promise<boolean> => {
    let targetPath = lastConfigPath;

    if (!targetPath) {
      const selected = await save({
        defaultPath: "config.json",
        filters: [{ name: "Config Files", extensions: ["json"] }],
      });
      if (!selected) return false;
      targetPath = selected;
    }

    try {
      const json = JSON.stringify(configRef.current, null, 2);
      await writeTextFile(targetPath, json);

      const storeInstance = await Store.load("config.json");
      await storeInstance.set("bridge_config", configRef.current);
      await storeInstance.set("last_config_path", targetPath);
      await storeInstance.save();
      setLastConfigPath(targetPath);
      savedConfigRef.current = configRef.current;
      return true;
    } catch (e) {
      console.error("Failed to save config to file", e);
      return false;
    }
  }, [lastConfigPath]);

  useEffect(() => {
    let cancelled = false;
    const unlisteners: (() => void)[] = [];

    // Each listener is unregistered immediately if the effect was already
    // torn down before its `listen()` call resolved (e.g. React StrictMode's
    // dev-mode mount/cleanup/remount), rather than being tracked in a
    // `cleanup` closure that may be assigned after cleanup already ran.
    const registerListener = (unlisten: Promise<() => void>): Promise<void> =>
      unlisten.then((fn) => {
        if (cancelled) {
          fn();
        } else {
          unlisteners.push(fn);
        }
      });

    const setupListeners = async () => {
      // Log listener
      await registerListener(
        listen<LogEntry>("bridge-log", (event) => {
          setLogs((prev) => [...prev, event.payload]);
        }),
      );

      // Stopped listener
      await registerListener(
        listen("bridge-stopped", () => {
          setIsRunning(false);
          setLogs((prev) => [
            ...prev,
            { level: "WARN", message: "Bridge stopped." },
          ]);
          void setWasRunning(false);
        }),
      );

      // File open listener
      await registerListener(
        listen("file-open", async () => {
          const selected = await open({
            multiple: false,
            filters: [{ name: "Config Files", extensions: ["json"] }],
          });

          if (!selected || Array.isArray(selected)) return;

          try {
            const text = await readTextFile(selected);
            const data = JSON.parse(text) as BridgeConfig;
            const merged = { ...configRef.current, ...data };
            setConfig(merged);
            savedConfigRef.current = merged;

            const storeInstance = await Store.load("config.json");
            await storeInstance.set("bridge_config", data);
            await storeInstance.set("last_config_path", selected);
            await storeInstance.save();
            setLastConfigPath(selected);
          } catch (e) {
            console.error("Failed to load config from file", e);
          }
        }),
      );

      // File save listener
      await registerListener(
        listen("file-save", async () => {
          await saveConfig();
        }),
      );
    };

    setupListeners();

    return () => {
      cancelled = true;
      unlisteners.forEach((fn) => fn());
    };
  }, [lastConfigPath, saveConfig]);

  // Intercept window close (red button, Cmd+Q / menu Quit — both route
  // through `window.close()`, see src-tauri/src/main.rs) so we can prompt to
  // save unsaved changes instead of exiting silently.
  useEffect(() => {
    let cancelled = false;
    let unlistenFn: (() => void) | undefined;

    getCurrentWindow()
      .onCloseRequested((event) => {
        const isDirty =
          JSON.stringify(configRef.current) !==
          JSON.stringify(savedConfigRef.current);
        if (isDirty) {
          event.preventDefault();
          setShowQuitPrompt(true);
        }
      })
      .then((fn) => {
        if (cancelled) {
          fn();
        } else {
          unlistenFn = fn;
        }
      });

    return () => {
      cancelled = true;
      unlistenFn?.();
    };
  }, []);

  const handleQuitSave = async () => {
    const ok = await saveConfig();
    setShowQuitPrompt(false);
    if (ok) {
      await getCurrentWindow().destroy();
    }
  };

  const handleQuitDiscard = async () => {
    setShowQuitPrompt(false);
    await getCurrentWindow().destroy();
  };

  const handleQuitCancel = () => {
    setShowQuitPrompt(false);
  };

  // Persists whether the bridge is intended to be running, so that if the
  // app quits (or crashes) while connected, the next launch can reconnect
  // automatically. Only cleared on an explicit stop or an unexpected
  // disconnect — never on quit itself — so it reflects the running state at
  // the moment the app last exited.
  const setWasRunning = async (value: boolean) => {
    try {
      const storeInstance = await Store.load("config.json");
      await storeInstance.set("was_running", value);
      await storeInstance.save();
    } catch (e) {
      console.error("Failed to persist bridge running state", e);
    }
  };

  const handleStart = async () => {
    setIsTransitioning(true);
    // Logged before the (async) start attempt so it's guaranteed to precede
    // any bridge-log events the backend emits while connecting — otherwise a
    // fast connection failure can log before this "success" confirmation,
    // reading as contradictory.
    setLogs((prev) => [
      ...prev,
      { level: "INFO", message: "Starting bridge..." },
    ]);
    try {
      const res = await invoke<BridgeResponse>("start_bridge", {
        config: configRef.current,
      });
      if (res.success) {
        setIsRunning(true);
        await setWasRunning(true);
      } else {
        setLogs((prev) => [...prev, { level: "ERROR", message: res.message }]);
      }
    } catch (e) {
      setLogs((prev) => [
        ...prev,
        { level: "ERROR", message: `Exception: ${e}` },
      ]);
    } finally {
      setIsTransitioning(false);
    }
  };

  const handleStop = async () => {
    setIsTransitioning(true);
    try {
      const res = await invoke<BridgeResponse>("stop_bridge");
      if (res.success) {
        setIsRunning(false);
        setLogs((prev) => [
          ...prev,
          { level: "INFO", message: `Bridge stopped: ${res.message}` },
        ]);
        await setWasRunning(false);
      } else {
        setLogs((prev) => [...prev, { level: "ERROR", message: res.message }]);
      }
    } catch (e) {
      setLogs((prev) => [
        ...prev,
        { level: "ERROR", message: `Exception: ${e}` },
      ]);
    } finally {
      setIsTransitioning(false);
    }
  };

  const updateConfig = (key: keyof BridgeConfig, value: string | number) => {
    setConfig((prev) => ({ ...prev, [key]: value }));
  };

  if (initializing) {
    return (
      <div className="flex items-center justify-center h-screen font-mono text-gray-500">
        Loading...
      </div>
    );
  }

  return (
    <div className="flex flex-col h-screen bg-slate-900 text-slate-200 font-mono p-4 gap-4">
      <header className="flex items-center justify-between border-b border-slate-700 pb-4">
        <h1 className="text-xl font-bold text-white">
          Yamaha RCP to OSC Bridge
        </h1>
        <div className="flex items-center gap-3">
          <span
            className={`px-3 py-1 rounded-full text-xs font-semibold ${isRunning ? "bg-green-500/20 text-green-400 border border-green-500/50" : "bg-red-500/20 text-red-400 border border-red-500/50"}`}
          >
            {isRunning ? "RUNNING" : "STOPPED"}
          </span>
          {!isRunning ? (
            <button
              onClick={handleStart}
              disabled={isTransitioning}
              className="px-4 py-1 bg-green-600 hover:bg-green-500 disabled:bg-green-800 disabled:text-slate-400 disabled:cursor-not-allowed text-white rounded transition-colors text-sm font-bold"
            >
              {isTransitioning ? "STARTING…" : "START"}
            </button>
          ) : (
            <button
              onClick={handleStop}
              disabled={isTransitioning}
              className="px-4 py-1 bg-red-600 hover:bg-red-500 disabled:bg-red-800 disabled:text-slate-400 disabled:cursor-not-allowed text-white rounded transition-colors text-sm font-bold"
            >
              {isTransitioning ? "STOPPING…" : "STOP"}
            </button>
          )}
        </div>
      </header>

      <div className="flex flex-1 gap-4 overflow-hidden">
        {/* Configuration Section */}
        <section className="w-1/3 flex flex-col gap-4 bg-slate-800 p-4 rounded-lg border border-slate-700 overflow-y-auto">
          <h2 className="text-sm font-bold text-slate-400 uppercase tracking-wider">
            Configuration
          </h2>

          <div className="flex flex-col gap-3">
            <div className="flex flex-col gap-1">
              <label className="text-xs text-slate-500">Console IP</label>
              <input
                className="bg-slate-900 border border-slate-600 rounded px-2 py-1 text-sm focus:outline-none focus:border-blue-500"
                value={config.console_ip}
                onChange={(e) => updateConfig("console_ip", e.target.value)}
                placeholder="192.168.x.x"
              />
            </div>

            <div className="flex flex-col gap-1">
              <label className="text-xs text-slate-500">RCP Port</label>
              <input
                type="number"
                className="bg-slate-900 border border-slate-600 rounded px-2 py-1 text-sm focus:outline-none focus:border-blue-500"
                value={config.rcp_port}
                onChange={(e) =>
                  updateConfig("rcp_port", parseInt(e.target.value) || 0)
                }
              />
            </div>

            <div className="h-px bg-slate-700 my-2" />

            <div className="flex flex-col gap-1">
              <label className="text-xs text-slate-500">OSC Out Address</label>
              <input
                className="bg-slate-900 border border-slate-600 rounded px-2 py-1 text-sm focus:outline-none focus:border-blue-500"
                value={config.udp_osc_out_addr}
                onChange={(e) =>
                  updateConfig("udp_osc_out_addr", e.target.value)
                }
              />
            </div>

            <div className="flex flex-col gap-1">
              <label className="text-xs text-slate-500">OSC Out Port</label>
              <input
                type="number"
                className="bg-slate-900 border border-slate-600 rounded px-2 py-1 text-sm focus:outline-none focus:border-blue-500"
                value={config.udp_osc_out_port}
                onChange={(e) =>
                  updateConfig(
                    "udp_osc_out_port",
                    parseInt(e.target.value) || 0,
                  )
                }
              />
            </div>

            <div className="h-px bg-slate-700 my-2" />

            <div className="flex flex-col gap-1">
              <label className="text-xs text-slate-500">OSC In Address</label>
              <input
                className="bg-slate-900 border border-slate-600 rounded px-2 py-1 text-sm focus:outline-none focus:border-blue-500"
                value={config.udp_osc_in_addr}
                onChange={(e) =>
                  updateConfig("udp_osc_in_addr", e.target.value)
                }
              />
            </div>

            <div className="flex flex-col gap-1">
              <label className="text-xs text-slate-500">OSC In Port</label>
              <input
                type="number"
                className="bg-slate-900 border border-slate-600 rounded px-2 py-1 text-sm focus:outline-none focus:border-blue-500"
                value={config.udp_osc_in_port}
                onChange={(e) =>
                  updateConfig("udp_osc_in_port", parseInt(e.target.value) || 0)
                }
              />
            </div>
          </div>

          {lastConfigPath && (
            <div className="mt-auto pt-4 text-[10px] text-slate-500 truncate">
              Config: {lastConfigPath}
            </div>
          )}
        </section>

        {/* Logs Section */}
        <section className="flex-1 flex flex-col bg-black rounded-lg border border-slate-700 overflow-hidden">
          <div className="bg-slate-800 px-4 py-2 border-b border-slate-700 flex justify-between items-center">
            <h2 className="text-xs font-bold text-slate-400 uppercase tracking-wider">
              Bridge Logs
            </h2>
            <button
              onClick={() => setLogs([])}
              className="text-[10px] text-slate-500 hover:text-white transition-colors"
            >
              CLEAR LOGS
            </button>
          </div>
          <div className="flex-1 overflow-y-auto p-4 text-xs leading-relaxed">
            {logs.length === 0 ? (
              <div className="text-slate-600 italic">No logs available...</div>
            ) : (
              <div className="flex flex-col gap-1">
                {logs.map((log, i) => (
                  <div
                    key={i}
                    className="border-l-2 border-slate-800 pl-2 hover:border-blue-500 transition-colors"
                  >
                    <span className="text-slate-600 mr-2">
                      [{new Date().toLocaleTimeString()}]
                    </span>
                    <span
                      className={`mr-2 font-bold ${LOG_LEVEL_COLORS[log.level]}`}
                    >
                      {log.level}
                    </span>
                    <span className={LOG_LEVEL_COLORS[log.level]}>
                      {log.message}
                    </span>
                  </div>
                ))}
                <div ref={logsEndRef} />
              </div>
            )}
          </div>
        </section>
      </div>

      {showQuitPrompt && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
          <div className="w-80 flex flex-col gap-4 bg-slate-800 border border-slate-700 rounded-lg p-5">
            <h2 className="text-sm font-bold text-white">Unsaved Changes</h2>
            <p className="text-xs text-slate-400">
              Your configuration has changed since it was last saved. Save
              before quitting?
            </p>
            <div className="flex justify-end gap-2 mt-2">
              <button
                onClick={handleQuitCancel}
                className="px-3 py-1 bg-slate-700 hover:bg-slate-600 text-slate-200 rounded transition-colors text-xs font-bold"
              >
                Cancel
              </button>
              <button
                onClick={handleQuitDiscard}
                className="px-3 py-1 bg-red-600 hover:bg-red-500 text-white rounded transition-colors text-xs font-bold"
              >
                Don&apos;t Save
              </button>
              <button
                onClick={handleQuitSave}
                className="px-3 py-1 bg-green-600 hover:bg-green-500 text-white rounded transition-colors text-xs font-bold"
              >
                Save
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
