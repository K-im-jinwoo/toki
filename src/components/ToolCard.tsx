import type { QuotaWindow, ToolSnapshot } from '../types/usage';
import { mapAgentStatusToMascot, STATUS_META, TOOL_META } from '../utils/format';
import { PastelToki } from './PastelToki';
import { QuotaBar } from './QuotaBar';

interface ToolCardProps {
  snapshot: ToolSnapshot;
  nowUnix: number;
}

export function ToolCard({ snapshot, nowUnix }: ToolCardProps) {
  const toolMeta = TOOL_META[snapshot.tool];
  const statusMeta = STATUS_META[snapshot.status];
  const miniState = snapshot.installed
    ? mapAgentStatusToMascot(snapshot.status)
    : 'sleep';

  const unconfiguredHint = !snapshot.installed
    ? '미설치'
    : snapshot.hookConfigured === false
      ? 'Hook 미설정'
      : snapshot.hookConfigured === true
        ? 'CLI 재시작·1회 응답 대기'
        : '데이터 없음';

  const groups = new Map<string, QuotaWindow[]>();
  for (const quota of snapshot.quotas) {
    const name = quota.group ?? '';
    const group = groups.get(name);
    if (group) group.push(quota);
    else groups.set(name, [quota]);
  }
  const showGroupNames = groups.size > 1;

  return (
    <div className="rounded-xl bg-white/55 dark:bg-white/[0.06] border border-black/6 dark:border-white/[0.08] px-3 py-2.5 flex items-center gap-2.5">
      <PastelToki state={miniState} theme={snapshot.tool} size={44} />

      <div className="flex-1 min-w-0">
        <div className="flex items-center justify-between gap-2 mb-1.5">
          <div className="flex items-center gap-1.5 min-w-0">
            <span className="text-xs font-semibold text-slate-800 dark:text-zinc-100 truncate">
              {toolMeta.name}
            </span>
            {snapshot.hookConfigured === false && (
              <span className="px-1.5 py-0.2 text-[9px] font-semibold rounded bg-amber-500/15 text-amber-700 border border-amber-500/25 dark:text-amber-300">
                Hook 필요
              </span>
            )}
          </div>

          <span
            className={`inline-flex items-center gap-1 px-1.5 py-0.5 rounded-full text-[10px] font-semibold border ${statusMeta.pillClass}`}
          >
            <span className="relative flex h-1.5 w-1.5">
              <span
                className={`inline-flex h-1.5 w-1.5 rounded-full ${statusMeta.dotClass}`}
              />
            </span>
            {statusMeta.label}
          </span>
        </div>

        {groups.size === 0 ? (
          <QuotaBar
            label="Quota"
            window={null}
            nowUnix={nowUnix}
            unconfiguredHint={unconfiguredHint}
          />
        ) : (
          <div className={showGroupNames ? 'flex flex-col gap-2' : undefined}>
            {Array.from(groups, ([name, quotas]) => (
              <div key={name}>
                {showGroupNames && (
                  <div className="truncate text-[10px] font-semibold text-slate-400 dark:text-zinc-400 leading-none mb-1">
                    {name || '기본'}
                  </div>
                )}
                <div className="grid grid-cols-2 gap-x-2.5 gap-y-1.5">
                  {quotas.map((q, idx) => {
                    const spanFull =
                      !showGroupNames && quotas.length % 2 === 1 && idx === quotas.length - 1;
                    return (
                      <div key={q.id} className={spanFull ? 'col-span-2' : undefined}>
                        <QuotaBar label={q.label} window={q} nowUnix={nowUnix} />
                      </div>
                    );
                  })}
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
