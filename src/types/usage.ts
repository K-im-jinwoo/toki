export type ToolId = 'codex' | 'antigravity' | 'claude';

export type AgentStatus =
  | 'running'
  | 'waiting'
  | 'idle'
  | 'quota_low'
  | 'unavailable';

export type MascotState =
  | 'sleep'
  | 'idle'
  | 'run'
  | 'sprint'
  | 'alert'
  | 'exhausted';

export type DataMode = 'live' | 'mock';

export type MockScenario =
  | 'auto'
  | 'sleep'
  | 'idle'
  | 'run'
  | 'sprint'
  | 'alert'
  | 'exhausted';

export type AccentTheme = 'default' | ToolId;

export interface QuotaWindow {
  id: string;
  label: string;
  remainingPercent: number;
  usedPercent: number;
  resetsAt: number | null;
  windowMinutes: number;
}

export interface SessionSummary {
  id: string;
  title: string;
  workspace: string | null;
  status: AgentStatus;
  updatedAt: number;
}

export interface ToolSnapshot {
  tool: ToolId;
  installed: boolean;
  hookConfigured: boolean | null;
  status: AgentStatus;
  quotas: QuotaWindow[];
  activeSessions: SessionSummary[];
  lastUpdatedAt: number | null;
}

export interface UsageSnapshot {
  overallState: MascotState;
  tools: ToolSnapshot[];
  generatedAt: number;
}

export interface VisibleToolsConfig {
  codex: boolean;
  antigravity: boolean;
  claude: boolean;
}

export interface CustomPathsConfig {
  codexSessionsDir: string | null;
  antigravityCliDir: string | null;
  claudeDir: string | null;
}

export interface TokiConfig {
  visibleTools: VisibleToolsConfig;
  customPaths: CustomPathsConfig;
  notificationsEnabled: boolean;
  lowQuotaThresholdPercent: number;
}

export interface HookStatus {
  antigravityConfigured: boolean;
  claudeConfigured: boolean;
  hookScriptPath: string;
}
