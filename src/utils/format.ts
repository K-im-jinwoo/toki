import type { AgentStatus, MascotState, ToolId } from '../types/usage';

export const TOOL_META: Record<
  ToolId,
  { name: string; tag: string; badgeClass: string }
> = {
  codex: {
    name: 'Codex',
    tag: 'CDX',
    badgeClass:
      'bg-blue-500/12 text-blue-600 border-blue-500/20 dark:text-blue-300 dark:border-blue-400/25',
  },
  antigravity: {
    name: 'Antigravity',
    tag: 'AGY',
    badgeClass:
      'bg-violet-500/12 text-violet-600 border-violet-500/20 dark:text-violet-300 dark:border-violet-400/25',
  },
  claude: {
    name: 'Claude Code',
    tag: 'CLD',
    badgeClass:
      'bg-orange-500/12 text-orange-600 border-orange-500/20 dark:text-orange-300 dark:border-orange-400/25',
  },
};

export const STATUS_META: Record<
  AgentStatus,
  { label: string; dotClass: string; pillClass: string }
> = {
  running: {
    label: 'Running',
    dotClass: 'bg-emerald-500 animate-pulse',
    pillClass:
      'bg-emerald-500/12 text-emerald-700 border-emerald-500/20 dark:text-emerald-300 dark:border-emerald-400/25',
  },
  waiting: {
    label: 'Waiting !',
    dotClass: 'bg-rose-500 animate-ping',
    pillClass:
      'bg-rose-500/12 text-rose-700 border-rose-500/25 dark:text-rose-300 dark:border-rose-400/30',
  },
  idle: {
    label: 'Idle',
    dotClass: 'bg-slate-400 dark:bg-zinc-400',
    pillClass:
      'bg-black/5 text-slate-600 border-black/8 dark:bg-white/8 dark:text-zinc-300 dark:border-white/10',
  },
  quota_low: {
    label: 'Quota Low',
    dotClass: 'bg-amber-500',
    pillClass:
      'bg-amber-500/12 text-amber-700 border-amber-500/25 dark:text-amber-300 dark:border-amber-400/30',
  },
  unavailable: {
    label: 'Unavailable',
    dotClass: 'bg-slate-300 dark:bg-zinc-600',
    pillClass:
      'bg-black/4 text-slate-400 border-black/6 dark:bg-white/5 dark:text-zinc-500 dark:border-white/8',
  },
};

export function mapAgentStatusToMascot(status: AgentStatus): MascotState {
  switch (status) {
    case 'running':
      return 'run';
    case 'waiting':
      return 'alert';
    case 'quota_low':
      return 'exhausted';
    case 'unavailable':
      return 'sleep';
    case 'idle':
    default:
      return 'idle';
  }
}

export function formatResetCountdown(
  resetsAt: number | null,
  nowUnix: number,
): string {
  if (resetsAt == null) return '--';
  const diff = Math.max(0, resetsAt - nowUnix);
  if (diff === 0) return '곧 초기화';
  const days = Math.floor(diff / 86400);
  const hours = Math.floor((diff % 86400) / 3600);
  const mins = Math.floor((diff % 3600) / 60);
  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${mins}m`;
  return `${Math.max(1, mins)}m`;
}

export function formatRelativeTime(updatedAt: number, nowUnix: number): string {
  const diff = Math.max(0, nowUnix - updatedAt);
  if (diff < 15) return '방금';
  if (diff < 60) return `${diff}초 전`;
  const mins = Math.floor(diff / 60);
  if (mins < 60) return `${mins}분 전`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}시간 전`;
  return `${Math.floor(hours / 24)}일 전`;
}
