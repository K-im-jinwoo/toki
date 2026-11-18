import type { SessionSummary, ToolId, ToolSnapshot } from '../types/usage';
import { formatRelativeTime, STATUS_META, TOOL_META } from '../utils/format';

interface SessionListProps {
  tools: ToolSnapshot[];
  nowUnix: number;
}

interface FlatSession extends SessionSummary {
  tool: ToolId;
}

export function SessionList({ tools, nowUnix }: SessionListProps) {
  const sessions: FlatSession[] = tools
    .flatMap((t) =>
      t.activeSessions.map((s) => ({
        ...s,
        tool: t.tool,
      })),
    )
    .sort((a, b) => b.updatedAt - a.updatedAt)
    .slice(0, 4);

  return (
    <div className="rounded-xl bg-white/50 dark:bg-white/[0.05] border border-black/6 dark:border-white/[0.08] px-3 py-2">
      <div className="flex items-center justify-between mb-1">
        <span className="text-[11px] font-semibold text-slate-600 dark:text-zinc-300">
          활성 · 최근 세션
        </span>
        <span className="text-[10px] text-slate-400 dark:text-zinc-500">
          {sessions.length}개
        </span>
      </div>

      {sessions.length === 0 ? (
        <p className="text-[11px] text-slate-400 dark:text-zinc-500 py-1.5 text-center">
          진행 중인 세션이 없습니다.
        </p>
      ) : (
        <ul className="space-y-1">
          {sessions.map((session) => {
            const toolMeta = TOOL_META[session.tool];
            const statusMeta = STATUS_META[session.status];
            return (
              <li
                key={`${session.tool}-${session.id}`}
                className="flex items-center justify-between gap-2 text-[11px] py-0.5"
              >
                <div className="flex items-center gap-1.5 min-w-0">
                  <span
                    className={`w-1.5 h-1.5 rounded-full shrink-0 ${statusMeta.dotClass}`}
                  />
                  <span
                    className={`px-1 py-0.2 rounded text-[9px] font-bold border shrink-0 ${toolMeta.badgeClass}`}
                  >
                    {toolMeta.tag}
                  </span>
                  <span className="truncate text-slate-700 dark:text-zinc-200 font-medium">
                    {session.title}
                  </span>
                </div>
                <span className="text-[10px] text-slate-400 dark:text-zinc-400 shrink-0 tabular-nums">
                  {formatRelativeTime(session.updatedAt, nowUnix)}
                </span>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
