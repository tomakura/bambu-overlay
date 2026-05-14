export type ConnectionMode = "lan" | "cloud";
export type PrinterStatus =
  | "unknown"
  | "connecting"
  | "idle"
  | "preparing"
  | "printing"
  | "paused"
  | "finished"
  | "failed"
  | "offline";

export interface OverlayPreferences {
  alwaysOnTop: boolean;
  clickThrough: boolean;
  opacity: number;
  compact: boolean;
}

export interface PrinterProfile {
  id: string;
  name: string;
  mode: ConnectionMode;
  ip?: string;
  serial?: string;
  cloudAccount?: string;
  cloudDeviceId?: string;
  model?: string;
  product?: string;
  createdAt: string;
  updatedAt: string;
}

export interface ProfileSecretInput {
  lanAccessCode?: string;
}

export interface CloudLoginDevice {
  devId: string;
  name: string;
  online: boolean;
  printStatus?: string;
  modelName?: string;
  productName?: string;
}

export interface CloudLoginResult {
  requiresCode: boolean;
  sessionId?: string;
  devices: CloudLoginDevice[];
  message?: string;
}

export interface AmsSlot {
  amsId: number;
  trayId: number;
  label: string;
  filamentType?: string;
  subBrand?: string;
  color: string;
  remaining?: number;
  nozzleMin?: number;
  nozzleMax?: number;
  active: boolean;
  empty: boolean;
}

export interface AmsHumidity {
  amsId: number;
  humidityPercent: number;
}

export interface PrinterAlert {
  code: string;
  level: "info" | "warning" | "error";
  message: string;
}

export interface PrinterSnapshot {
  profileId?: string;
  printerName: string;
  mode: ConnectionMode;
  connected: boolean;
  lastUpdated?: string;
  status: PrinterStatus;
  rawState?: string;
  statusLabel: string;
  jobName?: string;
  progress: number;
  remainingMinutes?: number;
  currentLayer?: number;
  totalLayers?: number;
  nozzleTemp?: number;
  nozzleTarget?: number;
  bedTemp?: number;
  bedTarget?: number;
  chamberTemp?: number;
  chamberTarget?: number;
  speedPercent?: number;
  speedLevel?: number;
  wifiSignal?: string;
  amsHumidity: AmsHumidity[];
  amsSlots: AmsSlot[];
  alerts: PrinterAlert[];
}

export interface AppBootstrap {
  profiles: PrinterProfile[];
  selectedProfileId?: string;
  snapshot?: PrinterSnapshot;
  preferences: OverlayPreferences;
}
