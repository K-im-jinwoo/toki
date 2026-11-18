import type { QuotaWindow } from '../types/usage';
import { formatResetCountdown } from '../utils/format';

interface QuotaBarProps {
  label: string;
  window: QuotaWindow | null;
  nowUnix: number;
  unconfiguredHint?: string;
}

export function QuotaBar({
  label,
  window,
  nowUnix,
  unconfiguredHint,
}: QuotaBarProps) {
  if (!window) {
    return (
      <div className="flex flex-col gap-1">
        <div className="flex items-center justify-between text-[11px]">
          <span className="font-medium text-slate-500 dark:text-zinc-400">
            {label}
          </span>
          <span className="text-slate-400 dark:text-zinc-500">
            {unconfiguredHint ?? '데이터 없음'}
          </span>
        </div>
        <div className="h-1.5 w-full rounded-full bg-black/8 dark:bg-white/10 overflow-hidden" />
      </div>
    );
  }

  const pct = Math.max(0, Math.min(100, window.remainingPercent));
  const barColor =
    pct <= 10
      ? 'bg-rose-400'
      : pct <= 30
        ? 'bg-amber-400'
        : 'bg-emerald-400';

  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center justify-between text-[11px]">
        <div className="flex items-center gap-1.5">
          <span className="font-medium text-slate-500 dark:text-zinc-400">
            {label}
          </span>
          <span className="font-semibold text-slate-800 dark:text-zinc-100 tabular-nums">
            {pct.toFixed(0)}%
          </span>
        </div>
        <span className="text-[10px] text-slate-400 dark:text-zinc-400 tabular-nums">
          ↻ {formatResetCountdown(window.resetsAt, nowUnix)}
        </span>
      </div>
      <div className="h-1.5 w-full rounded-full bg-black/8 dark:bg-white/10 overflow-hidden">
        <div
          className={`h-full rounded-full transition-all duration-300 ${barColor}`}
          style={{ width: `${pct}%` }}
        />
      </div>
    </div>
  );
}
