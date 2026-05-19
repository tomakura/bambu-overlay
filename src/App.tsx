import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  AlertTriangle,
  ArrowDown,
  ArrowLeft,
  ArrowUp,
  Box,
  Check,
  ChevronDown,
  Flame,
  Gauge,
  Loader2,
  Maximize2,
  Minimize2,
  Minus,
  MonitorCog,
  Network,
  Pause,
  PlugZap,
  RefreshCw,
  Settings,
  Square,
  Trash2,
  Wifi,
  X,
} from "lucide-react";
import {
  type Dispatch,
  type FormEvent,
  type KeyboardEvent,
  type SetStateAction,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  AppBootstrap,
  CloudLoginDevice,
  CloudLoginResult,
  OverlayPreferences,
  PrinterAlert,
  PrinterProfile,
  PrinterSnapshot,
} from "./types";

type SettingsTab = "printer" | "display";
type ConnectionFormMode = "lan" | "cloud";

interface CloudFormState {
  account: string;
  password: string;
  code: string;
  codeRequested: boolean;
  sessionId?: string;
  devices: CloudLoginDevice[];
}

const emptyPreferences: OverlayPreferences = {
  alwaysOnTop: true,
  clickThrough: false,
  opacity: 0.94,
  compact: false,
};

const emptyCloudFormState: CloudFormState = {
  account: "",
  password: "",
  code: "",
  codeRequested: false,
  sessionId: undefined,
  devices: [],
};

function cx(...parts: Array<string | false | undefined>) {
  return parts.filter(Boolean).join(" ");
}

function formatRemaining(minutes?: number) {
  if (minutes == null || Number.isNaN(minutes)) return "--";
  const safeMinutes = Math.max(0, Math.round(minutes));
  if (safeMinutes < 60) return `${safeMinutes}分`;
  const hours = Math.floor(safeMinutes / 60);
  const mins = safeMinutes % 60;
  return `${hours}時間${mins.toString().padStart(2, "0")}分`;
}

function formatFinishTime(minutes?: number) {
  if (minutes == null || Number.isNaN(minutes)) return "完了 --:--";
  const now = new Date();
  const finish = new Date(now.getTime() + Math.max(0, Math.round(minutes)) * 60_000);
  const hhmm = `${finish.getHours().toString().padStart(2, "0")}:${finish.getMinutes().toString().padStart(2, "0")}`;
  if (finish.toDateString() === now.toDateString()) return `完了 ${hhmm}`;
  return `完了 ${finish.getMonth() + 1}/${finish.getDate()} ${hhmm}`;
}

function formatCompactFinishTime(minutes?: number) {
  if (minutes == null || Number.isNaN(minutes)) return "--/-- --:--完了予定";
  const finish = new Date(Date.now() + Math.max(0, Math.round(minutes)) * 60_000);
  const hhmm = `${finish.getHours().toString().padStart(2, "0")}:${finish.getMinutes().toString().padStart(2, "0")}`;
  return `${finish.getMonth() + 1}/${finish.getDate()} ${hhmm}完了予定`;
}

function formatLayerCount(current?: number, total?: number) {
  const currentText = current == null ? "--" : current.toLocaleString();
  const totalText = total == null ? "--" : total.toLocaleString();
  return `${currentText}/${totalText}`;
}

function formatSimpleTemp(current?: number) {
  return current == null ? "--°C" : `${Math.round(current)}°C`;
}

function formatTarget(target?: number) {
  if (!target || target <= 0) return undefined;
  return `/${Math.round(target)}°`;
}

type TempTrend = "heating" | "cooling" | "stable" | "unknown";

function temperatureTrend(current?: number, target?: number): TempTrend {
  if (current == null || target == null || target <= 0) return "unknown";
  if (current < target - 1) return "heating";
  if (current > target + 1) return "cooling";
  return "stable";
}

function formatFreshness(lastUpdated: string | undefined, nowMs: number) {
  if (!lastUpdated) return { text: "更新 --", stale: false };
  const updated = new Date(lastUpdated).getTime();
  if (Number.isNaN(updated)) return { text: "更新 --", stale: false };
  const stale = nowMs - updated > 120_000;
  const d = new Date(updated);
  const hh = d.getHours().toString().padStart(2, "0");
  const mm = d.getMinutes().toString().padStart(2, "0");
  const ss = d.getSeconds().toString().padStart(2, "0");
  return { text: `更新 ${hh}:${mm}:${ss}`, stale };
}

function statusTone(snapshot?: PrinterSnapshot) {
  if (snapshot?.status === "connecting") return "connecting";
  if (!snapshot?.connected) return "offline";
  if (snapshot.alerts.some((alert) => alert.level === "error")) return "failed";
  return snapshot.status;
}

function topAlert(alerts: PrinterAlert[]) {
  return alerts.find((alert) => alert.level === "error") ?? alerts.find((alert) => alert.level === "warning") ?? alerts[0];
}

function alertText(alert?: PrinterAlert, total = 0) {
  if (!alert) return undefined;
  return total > 1 ? `${alert.message} +${total - 1}` : alert.message;
}

function amsHumidityLabel(snapshot: PrinterSnapshot) {
  if (!snapshot.amsHumidity.length) return undefined;
  const multi = snapshot.amsHumidity.length > 1;
  return snapshot.amsHumidity
    .map((unit) => multi ? `AMS${unit.amsId + 1} 湿度${unit.humidityPercent}%` : `湿度${unit.humidityPercent}%`)
    .join(" / ");
}

function slotDetail(slot: PrinterSnapshot["amsSlots"][number]) {
  if (slot.empty) return "Empty";
  const parts = [`AMS${slot.amsId + 1}-${slot.trayId + 1}`];
  if (slot.subBrand) parts.push(slot.subBrand);
  return parts.join(" · ");
}

function slotTypeLabel(slot: PrinterSnapshot["amsSlots"][number]) {
  if (slot.empty) return "Empty";
  return slot.filamentType || "Filament";
}

function measuredOverlayContentHeight(overlay: HTMLElement) {
  return Array.from(overlay.children).reduce((height, child) => {
    if (!(child instanceof HTMLElement)) return height;
    const style = window.getComputedStyle(child);
    if (style.display === "none" || style.position === "fixed" || style.position === "absolute") {
      return height;
    }
    const marginTop = style.marginTop === "auto" ? 0 : Number.parseFloat(style.marginTop) || 0;
    const marginBottom = style.marginBottom === "auto" ? 0 : Number.parseFloat(style.marginBottom) || 0;
    return height + child.getBoundingClientRect().height + marginTop + marginBottom;
  }, 0);
}

async function safeInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(command, args);
}

function offlineSnapshot(profile?: PrinterProfile): PrinterSnapshot {
  return {
    profileId: profile?.id,
    printerName: profile?.name || "Bambu Printer",
    mode: profile?.mode || "lan",
    connected: false,
    lastUpdated: undefined,
    status: "offline",
    rawState: undefined,
    statusLabel: "未接続",
    jobName: undefined,
    progress: 0,
    remainingMinutes: undefined,
    currentLayer: undefined,
    totalLayers: undefined,
    nozzleTemp: undefined,
    nozzleTarget: undefined,
    bedTemp: undefined,
    bedTarget: undefined,
    chamberTemp: undefined,
    chamberTarget: undefined,
    speedPercent: undefined,
    speedLevel: undefined,
    wifiSignal: undefined,
    amsHumidity: [],
    amsSlots: [],
    alerts: [],
  };
}

export default function App() {
  const [profiles, setProfiles] = useState<PrinterProfile[]>([]);
  const [selectedProfileId, setSelectedProfileId] = useState<string | undefined>();
  const [snapshot, setSnapshot] = useState<PrinterSnapshot | undefined>();
  const [preferences, setPreferences] = useState<OverlayPreferences>(emptyPreferences);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsTab, setSettingsTab] = useState<SettingsTab>("printer");
  const [cloudForm, setCloudForm] = useState<CloudFormState>(emptyCloudFormState);
  const [amsExpanded, setAmsExpanded] = useState(true);
  const [nowMs, setNowMs] = useState(() => Date.now());
  const [bootstrapped, setBootstrapped] = useState(false);
  const [backendReady, setBackendReady] = useState(true);
  const [busy, setBusy] = useState(false);
  const [toast, setToast] = useState<string | undefined>();
  const [persistentError, setPersistentError] = useState<string | undefined>();
  const [connectingProfileId, setConnectingProfileId] = useState<string | undefined>();
  const dragState = useRef<{ lastX: number; lastY: number } | undefined>();
  const overlayRef = useRef<HTMLElement>(null);
  const settingsButtonRef = useRef<HTMLButtonElement>(null);
  const lastCompactRef = useRef(emptyPreferences.compact);
  const autoConnectProfileRef = useRef<string | undefined>();

  useEffect(() => {
    const timer = window.setInterval(() => setNowMs(Date.now()), 30_000);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    safeInvoke<AppBootstrap>("get_app_bootstrap")
      .then((bootstrap) => {
        setProfiles(bootstrap.profiles);
        setSelectedProfileId(bootstrap.selectedProfileId);
        setSnapshot(bootstrap.snapshot);
        setPreferences(bootstrap.preferences);
        setSettingsOpen(bootstrap.profiles.length === 0);
        setBootstrapped(true);
      })
      .catch(() => {
        setBackendReady(false);
        setProfiles([]);
        setSelectedProfileId(undefined);
        setSnapshot(undefined);
        setPreferences(emptyPreferences);
        setSettingsOpen(false);
        setBootstrapped(true);
      });
  }, []);

  useEffect(() => {
    function moveWindow(event: MouseEvent) {
      const drag = dragState.current;
      if (!drag) return;
      const deltaX = Math.round(event.screenX - drag.lastX);
      const deltaY = Math.round(event.screenY - drag.lastY);
      drag.lastX = event.screenX;
      drag.lastY = event.screenY;
      if (deltaX === 0 && deltaY === 0) return;
      event.preventDefault();
      void safeInvoke("move_window_by", { deltaX, deltaY }).catch(() => undefined);
    }

    function stopDrag() {
      dragState.current = undefined;
    }

    window.addEventListener("mousemove", moveWindow);
    window.addEventListener("mouseup", stopDrag);
    window.addEventListener("blur", stopDrag);
    return () => {
      window.removeEventListener("mousemove", moveWindow);
      window.removeEventListener("mouseup", stopDrag);
      window.removeEventListener("blur", stopDrag);
    };
  }, []);

  const connectingProfileIdRef = useRef<string | undefined>(undefined);
  useEffect(() => {
    connectingProfileIdRef.current = connectingProfileId;
  }, [connectingProfileId]);

  useEffect(() => {
    if (!backendReady) return;
    let unlisten: (() => void) | undefined;
    let unlistenPrefs: (() => void) | undefined;
    listen<PrinterSnapshot>("snapshot-updated", (event) => {
      const nextSnapshot = event.payload;
      setSnapshot(nextSnapshot);
      const cid = connectingProfileIdRef.current;
      if (
        cid &&
        nextSnapshot.profileId === cid &&
        nextSnapshot.connected &&
        nextSnapshot.status !== "connecting"
      ) {
        setBusy(false);
        setConnectingProfileId(undefined);
        setPersistentError(undefined);
        setToast("接続しました");
      } else if (
        cid &&
        nextSnapshot.profileId === cid &&
        !nextSnapshot.connected &&
        nextSnapshot.status !== "connecting"
      ) {
        setBusy(false);
        setConnectingProfileId(undefined);
        setPersistentError(nextSnapshot.statusLabel || "接続に失敗しました");
      }
    }).then((dispose) => {
      unlisten = dispose;
    });
    listen<OverlayPreferences>("preferences-updated", (event) => {
      setPreferences(event.payload);
    }).then((dispose) => {
      unlistenPrefs = dispose;
    });

    return () => {
      unlisten?.();
      unlistenPrefs?.();
    };
  }, [backendReady]);

  useEffect(() => {
    if (!backendReady || !bootstrapped) return;
    safeInvoke("set_overlay_preferences", { preferences }).catch(() => undefined);
  }, [backendReady, bootstrapped, preferences]);

  useLayoutEffect(() => {
    if (!backendReady || !bootstrapped) return;
    const frame = window.requestAnimationFrame(() => {
      const contentHeight = settingsOpen || !overlayRef.current
        ? undefined
        : measuredOverlayContentHeight(overlayRef.current);
      safeInvoke("set_window_layout", {
        compact: preferences.compact,
        expanded: amsExpanded,
        settingsOpen,
        alertsCount: snapshot?.alerts.length ?? 0,
        contentHeight,
      }).catch(() => undefined);
    });
    return () => window.cancelAnimationFrame(frame);
  }, [
    backendReady,
    bootstrapped,
    preferences.compact,
    amsExpanded,
    settingsOpen,
    snapshot?.alerts.length,
    snapshot?.amsSlots.length,
    persistentError,
  ]);

  useEffect(() => {
    if (!lastCompactRef.current && preferences.compact) {
      setAmsExpanded(false);
    }
    lastCompactRef.current = preferences.compact;
  }, [preferences.compact]);

  useEffect(() => {
    if (!connectingProfileId) return;
    const timer = window.setTimeout(() => {
      setBusy(false);
      setPersistentError("接続確認がタイムアウトしました");
      setConnectingProfileId(undefined);
    }, 30_000);
    return () => window.clearTimeout(timer);
  }, [connectingProfileId]);

  const activeProfile = useMemo(
    () => profiles.find((profile) => profile.id === selectedProfileId) ?? profiles[0],
    [profiles, selectedProfileId],
  );

  const displaySnapshot = snapshot ?? offlineSnapshot(activeProfile);
  const tone = statusTone(displaySnapshot);
  const progress = Math.min(100, Math.max(0, displaySnapshot.progress ?? 0));
  const mainAlert = topAlert(displaySnapshot.alerts);
  const freshness = formatFreshness(displaySnapshot.lastUpdated, nowMs);
  const humidityLabel = amsHumidityLabel(displaySnapshot);
  const isFirstRun = bootstrapped && profiles.length === 0;

  useEffect(() => {
    if (!backendReady || !bootstrapped || isFirstRun || settingsOpen || busy || connectingProfileId) return;
    if (!activeProfile?.id) return;
    if (displaySnapshot.connected || displaySnapshot.status === "connecting") return;
    if (autoConnectProfileRef.current === activeProfile.id) return;
    autoConnectProfileRef.current = activeProfile.id;
    void connect(activeProfile.id);
  }, [
    activeProfile?.id,
    backendReady,
    bootstrapped,
    busy,
    connectingProfileId,
    displaySnapshot.connected,
    displaySnapshot.status,
    isFirstRun,
    settingsOpen,
  ]);

  async function connect(profileId = activeProfile?.id) {
    if (!profileId || !backendReady) return;
    setBusy(true);
    setPersistentError(undefined);
    setToast(undefined);
    setConnectingProfileId(profileId);
    try {
      const nextSnapshot = await safeInvoke<PrinterSnapshot>("connect_profile", { profileId });
      setSelectedProfileId(profileId);
      setSnapshot(nextSnapshot);
      if (nextSnapshot.connected && nextSnapshot.status !== "connecting") {
        setBusy(false);
        setConnectingProfileId(undefined);
        setToast("接続しました");
      }
    } catch (error) {
      const text = error instanceof Error ? error.message : String(error);
      if (text.includes("認証情報") || text.includes("安全な保存領域")) {
        setSettingsTab("printer");
        openSettings();
      }
      setPersistentError(text);
      setBusy(false);
      setConnectingProfileId(undefined);
    }
  }

  async function disconnect() {
    if (!backendReady) return;
    setBusy(true);
    setPersistentError(undefined);
    setConnectingProfileId(undefined);
    autoConnectProfileRef.current = undefined;
    try {
      await safeInvoke("disconnect_profile");
      setSnapshot((current) =>
        current ? { ...current, connected: false, status: "offline", statusLabel: "切断" } : current,
      );
      setToast("切断しました");
    } catch (error) {
      setPersistentError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  }

  async function refreshProfiles() {
    if (!backendReady) return;
    const bootstrap = await safeInvoke<AppBootstrap>("get_app_bootstrap");
    setProfiles(bootstrap.profiles);
    setSelectedProfileId(bootstrap.selectedProfileId);
    setSnapshot(bootstrap.snapshot);
    setPreferences(bootstrap.preferences);
  }

  async function deleteProfile(profileId: string) {
    if (!backendReady) return;
    setBusy(true);
    setPersistentError(undefined);
    try {
      await safeInvoke("delete_profile", { profileId });
      await refreshProfiles();
      setToast("プロファイルを削除しました");
    } catch (error) {
      setPersistentError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  }

  function openSettings() {
    setSettingsOpen(true);
  }

  function closeSettings() {
    setSettingsOpen(false);
    window.setTimeout(() => settingsButtonRef.current?.focus(), 0);
  }

  function closeApp() {
    void safeInvoke("close_window").catch(() => undefined);
  }

  function startWindowDrag(event: React.MouseEvent<HTMLElement>) {
    if (event.button !== 0) return;
    const target = event.target as HTMLElement;
    if (target.closest("button, input, textarea, select, a, label")) return;
    dragState.current = { lastX: event.screenX, lastY: event.screenY };
    event.preventDefault();
  }

  return (
    <main className={cx("shell", preferences.compact && "is-compact", amsExpanded && "ams-expanded")} style={{ opacity: preferences.opacity }}>
      <section ref={overlayRef} className={cx("overlay", `tone-${tone}`)}>
        <header className="overlay-header" onMouseDownCapture={startWindowDrag}>
          <div className="printer-identity">
            <div className="status-light" />
            <span className="state-pill">{displaySnapshot.statusLabel}</span>
            <h1 title={displaySnapshot.printerName || activeProfile?.name || "Bambu Printer"}>
              {displaySnapshot.printerName || activeProfile?.name || "Bambu Printer"}
            </h1>
          </div>
          {(busy || mainAlert) && (
            <div className="header-status">
              {busy && <Loader2 className="spin" size={14} aria-label="処理中" />}
              {mainAlert && (
                <span className={cx("alert-chip", mainAlert.level)} title={mainAlert.message}>
                  <AlertTriangle size={12} />
                  {alertText(mainAlert, displaySnapshot.alerts.length)}
                </span>
              )}
            </div>
          )}
          <div className="header-actions">
            <button
              className="icon-button"
              type="button"
              aria-label={preferences.compact ? "通常表示に戻す" : "コンパクト表示"}
              aria-pressed={preferences.compact}
              title={preferences.compact ? "通常表示に戻す" : "コンパクト表示"}
              onClick={() => setPreferences({ ...preferences, compact: !preferences.compact })}
            >
              {preferences.compact ? <Maximize2 size={17} /> : <Minimize2 size={17} />}
            </button>
            {displaySnapshot.amsSlots.length > 0 && (
              <button
                className="icon-button"
                type="button"
                aria-label={amsExpanded ? "AMS情報を格納" : "AMS情報を展開"}
                aria-pressed={amsExpanded}
                title={amsExpanded ? "AMS情報を格納" : "AMS情報を展開"}
                onClick={() => setAmsExpanded((value) => !value)}
              >
                <ChevronDown className={cx("ams-chevron", amsExpanded && "rotate")} size={17} />
              </button>
            )}
            <button
              ref={settingsButtonRef}
              className="icon-button"
              type="button"
              aria-label="設定"
              title="設定"
              onClick={openSettings}
            >
              <Settings size={17} />
            </button>
            <button className="icon-button" type="button" aria-label="閉じる" title="閉じる" onClick={closeApp}>
              <X size={17} />
            </button>
          </div>
        </header>

        <div className="progress-block">
          <div className="progress-bar-wrap">
            <span className="progress-label">
              {Math.round(progress)}<small>%</small>
            </span>
            <div className="progress-track">
              <div className="progress-fill" style={{ width: `${progress}%` }} />
            </div>
          </div>
          <div className="job-panel">
            <strong title={displaySnapshot.jobName || undefined}>
              {displaySnapshot.jobName || "ジョブなし"}
            </strong>
            <div className="job-meta">
              {preferences.compact ? (
                <span className="compact-summary">
                  残り{formatRemaining(displaySnapshot.remainingMinutes)}
                  （{formatCompactFinishTime(displaySnapshot.remainingMinutes)}）{" "}
                  {formatLayerCount(displaySnapshot.currentLayer, displaySnapshot.totalLayers)}
                </span>
              ) : (
                <>
                  <span className="meta-row">
                    <span className="meta-label">残り</span>
                    <span className="meta-value">{formatRemaining(displaySnapshot.remainingMinutes)}</span>
                  </span>
                  <span className="meta-row">
                    <span className="meta-label">完了</span>
                    <span className="meta-value">{formatFinishTime(displaySnapshot.remainingMinutes)}</span>
                  </span>
                  <span className="meta-row">
                    <span className="meta-label">Layer</span>
                    <span className="meta-value">{formatLayerCount(displaySnapshot.currentLayer, displaySnapshot.totalLayers)}</span>
                  </span>
                </>
              )}
            </div>
          </div>
        </div>

        {persistentError && (
          <div className="persistent-error" role="alert">
            <AlertTriangle size={15} />
            <span>{persistentError}</span>
            <button type="button" onClick={() => connect()} disabled={!activeProfile || busy}>
              再接続
            </button>
            <button type="button" onClick={openSettings}>
              設定
            </button>
          </div>
        )}

        <div className="metrics-grid">
          <Metric
            icon={<Flame size={16} />}
            label="Nozzle"
            value={formatSimpleTemp(displaySnapshot.nozzleTemp)}
            target={formatTarget(displaySnapshot.nozzleTarget)}
            trend={temperatureTrend(displaySnapshot.nozzleTemp, displaySnapshot.nozzleTarget)}
            heatRatio={
              displaySnapshot.nozzleTemp != null && (displaySnapshot.nozzleTarget ?? 0) > 0
                ? displaySnapshot.nozzleTemp / displaySnapshot.nozzleTarget!
                : undefined
            }
          />
          <Metric
            icon={<Square size={16} />}
            label="Bed"
            value={formatSimpleTemp(displaySnapshot.bedTemp)}
            target={formatTarget(displaySnapshot.bedTarget)}
            trend={temperatureTrend(displaySnapshot.bedTemp, displaySnapshot.bedTarget)}
            heatRatio={
              displaySnapshot.bedTemp != null && (displaySnapshot.bedTarget ?? 0) > 0
                ? displaySnapshot.bedTemp / displaySnapshot.bedTarget!
                : undefined
            }
          />
          <Metric
            icon={<Box size={16} />}
            label="Chamber"
            value={formatSimpleTemp(displaySnapshot.chamberTemp)}
            target={formatTarget(displaySnapshot.chamberTarget)}
            trend={temperatureTrend(displaySnapshot.chamberTemp, displaySnapshot.chamberTarget)}
          />
          <Metric icon={<Gauge size={16} />} label="Speed" value={`${displaySnapshot.speedPercent ?? "--"}%`} />
        </div>

        {amsExpanded && (
          <>
            <section className="ams-panel">
              <div className="section-title">
                <span>AMS</span>
                <small>{humidityLabel ?? (displaySnapshot.amsSlots.length === 0 ? "not detected" : undefined)}</small>
              </div>
              <div className="ams-grid">
                {displaySnapshot.amsSlots.length === 0 ? (
                  <div className="empty-ams">AMS情報なし</div>
                ) : (
                  displaySnapshot.amsSlots.map((slot) => (
                    <div key={`${slot.amsId}-${slot.trayId}`} className={cx("ams-slot", slot.active && "active", slot.empty && "empty")}>
                      <span className="swatch" style={{ backgroundColor: slot.color || "#1f2933" }} />
                      <div className="ams-slot-info">
                        <strong>{slotTypeLabel(slot)}</strong>
                        <small title={slotDetail(slot)}>{slotDetail(slot)}</small>
                      </div>
                    </div>
                  ))
                )}
              </div>
            </section>

            {displaySnapshot.alerts.length > 0 && (
              <section className="alerts">
                {displaySnapshot.alerts.map((alert) => (
                  <div key={alert.code} className={cx("alert-row", alert.level)}>
                    <AlertTriangle size={15} />
                    <span>{alert.message}</span>
                  </div>
                ))}
              </section>
            )}
          </>
        )}

        <footer className="footer-bar">
          <span className={cx("freshness", freshness.stale && "stale")}>{freshness.text}</span>
          <div className="footer-right">
            {displaySnapshot.wifiSignal && (
              <span className="wifi-badge" title={displaySnapshot.wifiSignal}>
                <Wifi size={11} />
              </span>
            )}
            {!displaySnapshot.connected && displaySnapshot.status !== "connecting" && !isFirstRun && (
              <button
                type="button"
                className="footer-reconnect"
                onClick={() => connect()}
                disabled={busy || !activeProfile}
                title="再接続"
              >
                {busy ? <Loader2 className="spin" size={11} /> : <PlugZap size={11} />}
                再接続
              </button>
            )}
          </div>
        </footer>
      </section>

      {!backendReady && <div className="dev-banner">Tauri未接続</div>}
      {toast && (
        <div className="toast" role="status" aria-live="polite" onAnimationEnd={() => setToast(undefined)}>
          {toast}
        </div>
      )}

      {settingsOpen && (
        <SettingsPanel
          tab={settingsTab}
          setTab={setSettingsTab}
          profiles={profiles}
          selectedProfileId={selectedProfileId}
          snapshot={displaySnapshot}
          busy={busy}
          preferences={preferences}
          setPreferences={setPreferences}
          cloudForm={cloudForm}
          setCloudForm={setCloudForm}
          backendReady={backendReady}
          firstRun={isFirstRun}
          onClose={closeSettings}
          onStartWindowDrag={startWindowDrag}
          onRefresh={refreshProfiles}
          onConnect={connect}
          onDisconnect={disconnect}
          onDeleteProfile={deleteProfile}
          setToast={setToast}
          setPersistentError={setPersistentError}
        />
      )}
    </main>
  );
}

function Metric({ icon, label, value, target, trend = "unknown", heatRatio }: {
  icon: React.ReactNode;
  label: string;
  value: string;
  target?: string;
  trend?: TempTrend;
  heatRatio?: number;
}) {
  return (
    <div className="metric">
      {icon}
      <div>
        <small>{label}</small>
        <strong>
          {value}
          {target
            ? <span className="metric-target">{target}</span>
            : <TempTrendIndicator trend={trend} />}
        </strong>
        {heatRatio != null && (
          <div className="heat-bar">
            <div className="heat-fill" style={{ width: `${Math.min(100, heatRatio * 100)}%` }} />
          </div>
        )}
      </div>
    </div>
  );
}

function TempTrendIndicator({ trend }: { trend: TempTrend }) {
  if (trend === "unknown") return null;
  const Icon = trend === "heating" ? ArrowUp : trend === "cooling" ? ArrowDown : Minus;
  const label = trend === "heating" ? "昇温中" : trend === "cooling" ? "降温中" : "保持中";
  return (
    <span className={cx("temp-trend", trend)} aria-label={label} title={label}>
      <Icon size={12} />
    </span>
  );
}

function SettingsPanel({
  tab,
  setTab,
  profiles,
  selectedProfileId,
  snapshot,
  busy,
  preferences,
  setPreferences,
  cloudForm,
  setCloudForm,
  backendReady,
  firstRun,
  onClose,
  onStartWindowDrag,
  onRefresh,
  onConnect,
  onDisconnect,
  onDeleteProfile,
  setToast,
  setPersistentError,
}: {
  tab: SettingsTab;
  setTab: (tab: SettingsTab) => void;
  profiles: PrinterProfile[];
  selectedProfileId?: string;
  snapshot?: PrinterSnapshot;
  busy: boolean;
  preferences: OverlayPreferences;
  setPreferences: (preferences: OverlayPreferences) => void;
  cloudForm: CloudFormState;
  setCloudForm: Dispatch<SetStateAction<CloudFormState>>;
  backendReady: boolean;
  firstRun: boolean;
  onClose: () => void;
  onStartWindowDrag: (event: React.MouseEvent<HTMLElement>) => void;
  onRefresh: () => Promise<void>;
  onConnect: (profileId?: string) => Promise<void>;
  onDisconnect: () => Promise<void>;
  onDeleteProfile: (profileId: string) => Promise<void>;
  setToast: (message?: string) => void;
  setPersistentError: (message?: string) => void;
}) {
  const dialogRef = useRef<HTMLDivElement>(null);
  const [connectionMode, setConnectionMode] = useState<ConnectionFormMode>(() => {
    const active = profiles.find((p) => p.id === selectedProfileId) ?? profiles[0];
    return active?.mode ?? "lan";
  });

  useEffect(() => {
    const firstFocusable = dialogRef.current?.querySelector<HTMLElement>(
      'button, input, select, textarea, [href], [tabindex]:not([tabindex="-1"])',
    );
    firstFocusable?.focus();
  }, []);

  function trapFocus(event: KeyboardEvent<HTMLDivElement>) {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
      return;
    }
    if (event.key !== "Tab") return;
    const focusable = Array.from(
      event.currentTarget.querySelectorAll<HTMLElement>(
        'button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [href], [tabindex]:not([tabindex="-1"])',
      ),
    ).filter((element) => element.offsetParent !== null);
    if (focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  return (
    <aside className="settings-panel" role="dialog" aria-modal="true" aria-labelledby="settings-title" onKeyDown={trapFocus}>
      <div ref={dialogRef} className="settings-card">
        <header className="settings-header" onMouseDownCapture={onStartWindowDrag}>
          <div>
            <p className="eyebrow">Bambu Overlay</p>
            <h2 id="settings-title">設定</h2>
          </div>
          <button className="icon-button" type="button" aria-label="設定を閉じる" onClick={onClose}>
            <X size={18} />
          </button>
        </header>

        <div className="settings-body">

        {firstRun && (
          <div className="first-run-note">
            <strong>最初のプリンターを追加</strong>
            <span>接続方式を選び、必要なときだけ追加フォームを開きます。</span>
          </div>
        )}

        <nav className="tabs" aria-label="設定カテゴリ">
          <button className={cx(tab === "printer" && "active")} type="button" onClick={() => setTab("printer")}>
            <Network size={16} />
            プリンター
          </button>
          <button className={cx(tab === "display" && "active")} type="button" onClick={() => setTab("display")}>
            <MonitorCog size={16} />
            表示
          </button>
        </nav>

        {tab === "printer" && (
          <>
            <div className="connection-bar">
              <span className={cx("conn-dot", snapshot?.connected ? "connected" : "disconnected")} />
              <span className="conn-label">
                {snapshot?.connected ? snapshot.printerName : "未接続"}
              </span>
              {snapshot?.connected ? (
                <button type="button" onClick={onDisconnect} disabled={busy || !backendReady}>
                  <Pause size={13} />切断
                </button>
              ) : (
                <button type="button" onClick={() => onConnect()} disabled={busy || !backendReady || profiles.length === 0}>
                  <PlugZap size={13} />接続
                </button>
              )}
            </div>
            <div className="mode-switch" role="group" aria-label="接続方式">
              <button className={cx(connectionMode === "lan" && "active")} type="button" onClick={() => setConnectionMode("lan")}>
                LAN
              </button>
              <button className={cx(connectionMode === "cloud" && "active")} type="button" onClick={() => setConnectionMode("cloud")}>
                Cloud
              </button>
            </div>
            {connectionMode === "lan" ? (
              <LanForm
                backendReady={backendReady}
                profiles={profiles}
                selectedProfileId={selectedProfileId}
                firstRun={firstRun}
                onRefresh={onRefresh}
                onConnect={onConnect}
                onDeleteProfile={onDeleteProfile}
                setToast={setToast}
                setPersistentError={setPersistentError}
              />
            ) : (
              <CloudForm
                backendReady={backendReady}
                profiles={profiles}
                selectedProfileId={selectedProfileId}
                cloudForm={cloudForm}
                setCloudForm={setCloudForm}
                firstRun={firstRun}
                onRefresh={onRefresh}
                onConnect={onConnect}
                onDeleteProfile={onDeleteProfile}
                setToast={setToast}
                setPersistentError={setPersistentError}
              />
            )}
          </>
        )}
        {tab === "display" && (
          <DisplayForm
            preferences={preferences}
            setPreferences={setPreferences}
            backendReady={backendReady}
            setToast={setToast}
            setPersistentError={setPersistentError}
          />
        )}

        </div>
      </div>
    </aside>
  );
}

function LanForm({
  backendReady,
  profiles,
  selectedProfileId,
  firstRun,
  onRefresh,
  onConnect,
  onDeleteProfile,
  setToast,
  setPersistentError,
}: {
  backendReady: boolean;
  profiles: PrinterProfile[];
  selectedProfileId?: string;
  firstRun: boolean;
  onRefresh: () => Promise<void>;
  onConnect: (profileId?: string) => Promise<void>;
  onDeleteProfile: (profileId: string) => Promise<void>;
  setToast: (message?: string) => void;
  setPersistentError: (message?: string) => void;
}) {
  const [name, setName] = useState("");
  const [ip, setIp] = useState("");
  const [serial, setSerial] = useState("");
  const [accessCode, setAccessCode] = useState("");
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | undefined>();

  async function submit(event: FormEvent) {
    event.preventDefault();
    setFormError(undefined);
    setPersistentError(undefined);
    if (!backendReady) {
      setFormError("デスクトップ版で保存できます");
      return;
    }
    setSaving(true);
    try {
      const profile = await safeInvoke<PrinterProfile>("save_lan_profile", {
        input: {
          name,
          ip,
          serial,
          accessCode,
        },
      });
      await onRefresh();
      await onConnect(profile.id);
      setName("");
      setIp("");
      setSerial("");
      setAccessCode("");
      setToast("LANプロファイルを保存しました");
    } catch (error) {
      setFormError(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  }

  return (
    <form className="settings-form" onSubmit={submit}>
      {firstRun && <p className="helper-text">LANアクセスコードとDevice IDはプリンター本体のネットワーク/LAN設定画面で確認します。</p>}
      {formError && <div className="form-error" role="alert">{formError}</div>}
      <label>
        表示名
        <input value={name} onChange={(event) => setName(event.target.value)} placeholder="P1S 工房" required />
      </label>
      <label>
        プリンターIP
        <input value={ip} onChange={(event) => setIp(event.target.value)} placeholder="192.168.1.42" required />
      </label>
      <label>
        シリアル / Device ID
        <input value={serial} onChange={(event) => setSerial(event.target.value)} placeholder="01S00A..." required />
      </label>
      <label>
        LANアクセスコード
        <input value={accessCode} onChange={(event) => setAccessCode(event.target.value)} placeholder="8桁コード" type="password" required />
      </label>
      <button className="primary-button" type="submit" disabled={saving}>
        {saving ? "保存中..." : "保存して接続"}
      </button>
      <ProfileList
        profiles={profiles.filter((profile) => profile.mode === "lan")}
        selectedProfileId={selectedProfileId}
        onConnect={onConnect}
        onDelete={onDeleteProfile}
      />
    </form>
  );
}

function CloudForm({
  backendReady,
  profiles,
  selectedProfileId,
  cloudForm,
  setCloudForm,
  firstRun,
  onRefresh,
  onConnect,
  onDeleteProfile,
  setToast,
  setPersistentError,
}: {
  backendReady: boolean;
  profiles: PrinterProfile[];
  selectedProfileId?: string;
  cloudForm: CloudFormState;
  setCloudForm: Dispatch<SetStateAction<CloudFormState>>;
  firstRun: boolean;
  onRefresh: () => Promise<void>;
  onConnect: (profileId?: string) => Promise<void>;
  onDeleteProfile: (profileId: string) => Promise<void>;
  setToast: (message?: string) => void;
  setPersistentError: (message?: string) => void;
}) {
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | undefined>();
  const [adding, setAdding] = useState(false);
  const { account, password, code, codeRequested, sessionId, devices } = cloudForm;

  function updateCloudForm(patch: Partial<CloudFormState>) {
    setCloudForm((current) => ({ ...current, ...patch }));
  }

  async function requestCode() {
    if (!backendReady) {
      setFormError("デスクトップ版でCloud認証できます");
      return;
    }
    setSaving(true);
    setFormError(undefined);
    setPersistentError(undefined);
    try {
      const result = await safeInvoke<CloudLoginResult>("cloud_login", {
        input: {
          account,
          password,
          code: undefined,
        },
      });
      if (result.requiresCode) {
        updateCloudForm({
          codeRequested: true,
          devices: [],
          sessionId: undefined,
        });
        setToast(result.message || "認証コードを送信しました");
      } else {
        updateCloudForm({
          codeRequested: false,
          devices: result.devices,
          sessionId: result.sessionId,
        });
        setToast("Cloudログインしました");
      }
    } catch (error) {
      setFormError(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  }

  async function login(event: FormEvent) {
    event.preventDefault();
    if (!codeRequested) {
      await requestCode();
      return;
    }
    if (!backendReady) {
      setFormError("デスクトップ版でCloud認証できます");
      return;
    }
    setSaving(true);
    setFormError(undefined);
    setPersistentError(undefined);
    try {
      const trimmedCode = code.trim();
      const result = await safeInvoke<CloudLoginResult>("cloud_login", {
        input: {
          account,
          password: undefined,
          code: trimmedCode || undefined,
        },
      });
      updateCloudForm({
        codeRequested: false,
        devices: result.devices,
        sessionId: result.sessionId,
      });
      setToast("Cloudログインしました");
    } catch (error) {
      setFormError(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  }

  async function addDevice(device: CloudLoginDevice) {
    if (!sessionId) return;
    setSaving(true);
    setFormError(undefined);
    setPersistentError(undefined);
    try {
      const profile = await safeInvoke<PrinterProfile>("save_cloud_profile", {
        input: {
          sessionId,
          deviceId: device.devId,
          name: device.name,
        },
      });
      await onRefresh();
      await onConnect(profile.id);
      setCloudForm((current) => ({
        ...current,
        password: "",
        code: "",
        codeRequested: false,
        sessionId: undefined,
        devices: [],
      }));
      setAdding(false);
      setToast("Cloudプロファイルを保存しました");
    } catch (error) {
      setFormError(error instanceof Error ? error.message : String(error));
    } finally {
      setSaving(false);
    }
  }

  function backToPasswordLogin() {
    updateCloudForm({
      code: "",
      codeRequested: false,
      devices: [],
      sessionId: undefined,
    });
    setFormError(undefined);
  }

  if (!adding && !codeRequested && devices.length === 0) {
    return (
      <div className="settings-form">
        {firstRun && <p className="helper-text">Cloud接続が必要な場合だけアカウント入力を開きます。</p>}
        <button className="primary-button" type="button" onClick={() => setAdding(true)}>
          Cloudプリンターを追加
        </button>
        <ProfileList
          profiles={profiles.filter((profile) => profile.mode === "cloud")}
          selectedProfileId={selectedProfileId}
          onConnect={onConnect}
          onDelete={onDeleteProfile}
        />
      </div>
    );
  }

  if (devices.length > 0 && sessionId) {
    return (
      <div className="settings-form">
        {formError && <div className="form-error" role="alert">{formError}</div>}
        <div className="device-list">
          {devices.map((device) => (
            <button key={device.devId} type="button" onClick={() => addDevice(device)} disabled={saving || !device.online}>
              <span>{device.name}</span>
              <small>{[device.productName || device.modelName || device.devId, device.online ? device.printStatus || "online" : "offline"].filter(Boolean).join(" · ")}</small>
            </button>
          ))}
        </div>
        <button className="secondary-button" type="button" onClick={backToPasswordLogin} disabled={saving}>
          <ArrowLeft size={14} />
          戻る
        </button>
        <ProfileList
          profiles={profiles.filter((profile) => profile.mode === "cloud")}
          selectedProfileId={selectedProfileId}
          onConnect={onConnect}
          onDelete={onDeleteProfile}
        />
      </div>
    );
  }

  return (
    <form className="settings-form" onSubmit={login}>
      {firstRun && <p className="helper-text">Bambuアカウントへログインし、登録済みプリンターから追加します。</p>}
      {formError && <div className="form-error" role="alert">{formError}</div>}
      {codeRequested ? (
        <>
          <p className="helper-text">{account} 宛ての認証コードを入力してください。</p>
          <label>
            認証コード
            <input value={code} onChange={(event) => updateCloudForm({ code: event.target.value })} placeholder="6桁コード" autoFocus required />
          </label>
          <div className="form-actions three">
            <button className="secondary-button" type="button" onClick={backToPasswordLogin} disabled={saving}>
              <ArrowLeft size={14} />
              戻る
            </button>
            <button className="secondary-button" type="button" onClick={requestCode} disabled={saving || !account.trim() || !password.trim()}>
              <RefreshCw size={14} />
              再送
            </button>
            <button className="primary-button" type="submit" disabled={saving || !code.trim()}>
              {saving ? "認証中..." : "ログイン"}
            </button>
          </div>
        </>
      ) : (
        <>
          <label>
            Bambuアカウント
            <input
              value={account}
              onChange={(event) => {
                updateCloudForm({
                  account: event.target.value,
                  code: "",
                  codeRequested: false,
                  devices: [],
                  sessionId: undefined,
                });
              }}
              placeholder="mail@example.com"
              required
            />
          </label>
          <label>
            パスワード
            <input
              value={password}
              onChange={(event) => {
                updateCloudForm({
                  password: event.target.value,
                  code: "",
                  codeRequested: false,
                  devices: [],
                  sessionId: undefined,
                });
              }}
              type="password"
              required
            />
          </label>
          <button className="primary-button" type="submit" disabled={saving}>
            {saving ? "送信中..." : "認証コードを送信"}
          </button>
        </>
      )}

      {!codeRequested && (
        <ProfileList
          profiles={profiles.filter((profile) => profile.mode === "cloud")}
          selectedProfileId={selectedProfileId}
          onConnect={onConnect}
          onDelete={onDeleteProfile}
        />
      )}
    </form>
  );
}

function DisplayForm({
  preferences,
  setPreferences,
  backendReady,
  setToast,
  setPersistentError,
}: {
  preferences: OverlayPreferences;
  setPreferences: (preferences: OverlayPreferences) => void;
  backendReady: boolean;
  setToast: (message?: string) => void;
  setPersistentError: (message?: string) => void;
}) {
  const [confirmClickThrough, setConfirmClickThrough] = useState(false);
  const [formError, setFormError] = useState<string | undefined>();

  function update(next: OverlayPreferences) {
    setPreferences(next);
    setFormError(undefined);
    setPersistentError(undefined);
  }

  return (
    <div className="settings-form">
      {formError && <div className="form-error" role="alert">{formError}</div>}
      <label className="toggle-row">
        <span>常時最前面</span>
        <input
          type="checkbox"
          checked={preferences.alwaysOnTop}
          onChange={(event) => update({ ...preferences, alwaysOnTop: event.target.checked })}
        />
      </label>
      <label className="toggle-row">
        <span>クリック透過</span>
        <input
          type="checkbox"
          checked={preferences.clickThrough}
          onChange={(event) => {
            if (event.target.checked) {
              setConfirmClickThrough(true);
            } else {
              setConfirmClickThrough(false);
              void update({ ...preferences, clickThrough: false });
            }
          }}
        />
      </label>
      {confirmClickThrough && !preferences.clickThrough && (
        <div className="confirm-box">
          <span>有効にするとこのウィンドウはクリックを受け付けません。解除はトレイメニューの「クリック透過を解除」から行えます。</span>
          <div className="confirm-actions">
            <button type="button" className="secondary-button" onClick={() => setConfirmClickThrough(false)}>
              キャンセル
            </button>
            <button
              type="button"
              className="primary-button"
              onClick={() => {
                setConfirmClickThrough(false);
                void update({ ...preferences, clickThrough: true });
                setToast("クリック透過を有効にしました");
              }}
            >
              有効にする
            </button>
          </div>
        </div>
      )}
      <label className="toggle-row">
        <span>コンパクト</span>
        <input type="checkbox" checked={preferences.compact} onChange={(event) => update({ ...preferences, compact: event.target.checked })} />
      </label>
      <label>
        透明度 {Math.round(preferences.opacity * 100)}%
        <input
          type="range"
          min="0.55"
          max="1"
          step="0.01"
          value={preferences.opacity}
          onChange={(event) => update({ ...preferences, opacity: Number(event.target.value) })}
        />
      </label>
    </div>
  );
}

function ProfileList({
  profiles,
  selectedProfileId,
  onConnect,
  onDelete,
}: {
  profiles: PrinterProfile[];
  selectedProfileId?: string;
  onConnect: (profileId?: string) => Promise<void>;
  onDelete: (profileId: string) => Promise<void>;
}) {
  const [pendingDeleteId, setPendingDeleteId] = useState<string | undefined>();

  if (profiles.length === 0) return <div className="empty-list">保存済みプロファイルはありません</div>;
  return (
    <div className="profile-list">
      {profiles.map((profile) => {
        const pendingDelete = pendingDeleteId === profile.id;
        return (
          <div key={profile.id} className={cx("profile-row", profile.id === selectedProfileId && "active", pendingDelete && "confirm-delete")}>
            <button type="button" className="profile-connect" onClick={() => onConnect(profile.id)} disabled={pendingDelete}>
              <span>{profile.name}</span>
              <small>{profile.mode === "cloud" ? profile.cloudDeviceId : profile.ip}</small>
            </button>
            {pendingDelete ? (
              <div className="profile-confirm">
                <span>{profile.name} を削除しますか</span>
                <button type="button" onClick={() => setPendingDeleteId(undefined)}>
                  <X size={14} />
                </button>
                <button type="button" onClick={() => onDelete(profile.id)}>
                  <Check size={14} />
                </button>
              </div>
            ) : (
              <button className="profile-delete" type="button" aria-label={`${profile.name} を削除`} title="削除" onClick={() => setPendingDeleteId(profile.id)}>
                <Trash2 size={14} />
              </button>
            )}
          </div>
        );
      })}
    </div>
  );
}
