import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { HookStatus, TokiConfig, ToolId, UsageSnapshot } from '../types/usage';

interface SettingsViewProps {
  onSnapshotUpdated: (snapshot: UsageSnapshot) => void;
}

const TOOL_TOGGLES: { id: ToolId; label: string }[] = [
  { id: 'codex', label: 'Codex' },
  { id: 'antigravity', label: 'Antigravity' },
  { id: 'claude', label: 'Claude Code' },
];

export function SettingsView({ onSnapshotUpdated }: SettingsViewProps) {
  const [config, setConfig] = useState<TokiConfig>({
    visibleTools: { codex: true, antigravity: true, claude: true },
    customPaths: {
      codexSessionsDir: null,
      antigravityCliDir: null,
      claudeDir: null,
    },
    notificationsEnabled: true,
    lowQuotaThresholdPercent: 10,
  });
  const [hooks, setHooks] = useState<HookStatus | null>(null);
  const [autostart, setAutostart] = useState(false);
  const [codexPath, setCodexPath] = useState('');
  const [agyPath, setAgyPath] = useState('');
  const [claudePath, setClaudePath] = useState('');
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    invoke<TokiConfig>('get_config')
      .then((cfg) => {
        setConfig(cfg);
        setCodexPath(cfg.customPaths.codexSessionsDir ?? '');
        setAgyPath(cfg.customPaths.antigravityCliDir ?? '');
        setClaudePath(cfg.customPaths.claudeDir ?? '');
      })
      .catch(() => {});

    invoke<HookStatus>('get_hook_status').then(setHooks).catch(() => {});
    invoke<boolean>('get_autostart_enabled')
      .then(setAutostart)
      .catch(() => {});
  }, []);

  async function persistConfig(next: TokiConfig, feedback?: string) {
    setConfig(next);
    try {
      const updated = await invoke<UsageSnapshot>('save_config', {
        config: next,
      });
      onSnapshotUpdated(updated);
      if (feedback) setMessage(feedback);
    } catch (err) {
      setMessage(`저장 실패: ${String(err)}`);
    }
  }

  function handleToggleTool(tool: ToolId) {
    const next: TokiConfig = {
      ...config,
      visibleTools: {
        ...config.visibleTools,
        [tool]: !config.visibleTools[tool],
      },
    };
    void persistConfig(next);
  }

  async function handleHookAction(tool: ToolId, install: boolean) {
    try {
      const nextHooks = await invoke<HookStatus>(
        install ? 'install_hook' : 'uninstall_hook',
        { tool },
      );
      setHooks(nextHooks);
      const snap = await invoke<UsageSnapshot>('get_usage_snapshot');
      onSnapshotUpdated(snap);
      setMessage(
        install
          ? `${tool === 'antigravity' ? 'Antigravity' : 'Claude Code'} Hook 설치 완료`
          : `${tool === 'antigravity' ? 'Antigravity' : 'Claude Code'} Hook 해제 완료`,
      );
    } catch (err) {
      setMessage(`Hook 설정 오류: ${String(err)}`);
    }
  }

  async function handleAutostartToggle() {
    try {
      const next = await invoke<boolean>('set_autostart_enabled', {
        enabled: !autostart,
      });
      setAutostart(next);
    } catch (err) {
      setMessage(`자동 실행 설정 오류: ${String(err)}`);
    }
  }

  function handleSavePaths() {
    const normalize = (s: string) => {
      const t = s.trim();
      return t.length > 0 ? t : null;
    };
    const next: TokiConfig = {
      ...config,
      customPaths: {
        codexSessionsDir: normalize(codexPath),
        antigravityCliDir: normalize(agyPath),
        claudeDir: normalize(claudePath),
      },
    };
    void persistConfig(next, '경로 설정이 저장되었습니다.');
  }

  return (
    <div className="space-y-2 max-h-[430px] overflow-y-auto pr-0.5">
      {message && (
        <div className="px-2.5 py-1.5 rounded-lg bg-indigo-500/15 border border-indigo-500/25 text-[11px] text-indigo-700 dark:text-indigo-200 font-medium">
          {message}
        </div>
      )}

      {/* Visible Tools */}
      <section className="rounded-xl bg-white/55 dark:bg-white/[0.06] border border-black/6 dark:border-white/[0.08] p-2.5">
        <div className="text-[11px] font-semibold text-slate-700 dark:text-zinc-200 mb-1.5">
          표시할 도구 (카드 · 상태 계산 포함)
        </div>
        <div className="grid grid-cols-3 gap-1.5">
          {TOOL_TOGGLES.map((t) => {
            const on = config.visibleTools[t.id];
            return (
              <button
                key={t.id}
                type="button"
                onClick={() => handleToggleTool(t.id)}
                className={`px-2 py-1.5 rounded-lg text-[11px] font-semibold border transition-colors cursor-pointer ${
                  on
                    ? 'bg-indigo-500 text-white border-indigo-500 shadow-sm'
                    : 'bg-white/60 dark:bg-white/[0.04] text-slate-400 dark:text-zinc-500 border-black/8 dark:border-white/10 hover:text-slate-600 dark:hover:text-zinc-300'
                }`}
              >
                {t.label}
              </button>
            );
          })}
        </div>
      </section>

      {/* statusLine Hook Setup */}
      <section className="rounded-xl bg-white/55 dark:bg-white/[0.06] border border-black/6 dark:border-white/[0.08] p-2.5 space-y-2">
        <div className="text-[11px] font-semibold text-slate-700 dark:text-zinc-200">
          statusLine 훅 연동 (5h / Week / Argon)
        </div>

        <div className="flex items-center justify-between gap-2 text-[11px]">
          <div>
            <span className="font-medium text-slate-700 dark:text-zinc-200">
              Antigravity (agy)
            </span>
            <span
              className={`ml-1.5 text-[10px] font-semibold ${
                hooks?.antigravityConfigured
                  ? 'text-emerald-600 dark:text-emerald-400'
                  : 'text-amber-600 dark:text-amber-400'
              }`}
            >
              {hooks?.antigravityConfigured ? '연결됨' : '미설정'}
            </span>
          </div>
          <button
            type="button"
            onClick={() =>
              handleHookAction(
                'antigravity',
                !(hooks?.antigravityConfigured ?? false),
              )
            }
            className="px-2 py-1 rounded-md text-[10px] font-semibold bg-white/80 dark:bg-white/10 border border-black/10 dark:border-white/15 hover:border-indigo-400 text-slate-700 dark:text-zinc-100 cursor-pointer"
          >
            {hooks?.antigravityConfigured ? '해제' : '원클릭 설치'}
          </button>
        </div>

        <div className="flex items-center justify-between gap-2 text-[11px]">
          <div>
            <span className="font-medium text-slate-700 dark:text-zinc-200">
              Claude Code
            </span>
            <span
              className={`ml-1.5 text-[10px] font-semibold ${
                hooks?.claudeConfigured
                  ? 'text-emerald-600 dark:text-emerald-400'
                  : 'text-amber-600 dark:text-amber-400'
              }`}
            >
              {hooks?.claudeConfigured ? '연결됨' : '미설정'}
            </span>
          </div>
          <button
            type="button"
            onClick={() =>
              handleHookAction('claude', !(hooks?.claudeConfigured ?? false))
            }
            className="px-2 py-1 rounded-md text-[10px] font-semibold bg-white/80 dark:bg-white/10 border border-black/10 dark:border-white/15 hover:border-indigo-400 text-slate-700 dark:text-zinc-100 cursor-pointer"
          >
            {hooks?.claudeConfigured ? '해제' : '원클릭 설치'}
          </button>
        </div>

        <p className="text-[10px] text-slate-400 dark:text-zinc-400 leading-relaxed">
          기존 <code>statusLine</code> 명령어가 있으면 유지한 채 체인으로 연결합니다.
        </p>
      </section>

      {/* Notifications & Autostart */}
      <section className="rounded-xl bg-white/55 dark:bg-white/[0.06] border border-black/6 dark:border-white/[0.08] p-2.5 space-y-2">
        <div className="text-[11px] font-semibold text-slate-700 dark:text-zinc-200">
          알림 · 자동 실행
        </div>

        <div className="flex items-center justify-between text-[11px]">
          <span className="text-slate-600 dark:text-zinc-300">
            부팅 시 자동 실행
          </span>
          <button
            type="button"
            onClick={handleAutostartToggle}
            className={`px-2 py-0.5 rounded-full text-[10px] font-semibold border cursor-pointer ${
              autostart
                ? 'bg-emerald-500/15 text-emerald-700 border-emerald-500/25 dark:text-emerald-300'
                : 'bg-white/60 dark:bg-white/5 text-slate-400 dark:text-zinc-500 border-black/8 dark:border-white/10'
            }`}
          >
            {autostart ? 'ON' : 'OFF'}
          </button>
        </div>

        <div className="flex items-center justify-between text-[11px]">
          <span className="text-slate-600 dark:text-zinc-300">
            응답 대기(!) · 저잔여 알림
          </span>
          <button
            type="button"
            onClick={() =>
              persistConfig({
                ...config,
                notificationsEnabled: !config.notificationsEnabled,
              })
            }
            className={`px-2 py-0.5 rounded-full text-[10px] font-semibold border cursor-pointer ${
              config.notificationsEnabled
                ? 'bg-emerald-500/15 text-emerald-700 border-emerald-500/25 dark:text-emerald-300'
                : 'bg-white/60 dark:bg-white/5 text-slate-400 dark:text-zinc-500 border-black/8 dark:border-white/10'
            }`}
          >
            {config.notificationsEnabled ? 'ON' : 'OFF'}
          </button>
        </div>

        <div className="flex items-center justify-between text-[11px]">
          <span className="text-slate-600 dark:text-zinc-300">
            쿼터 경고 임계치
          </span>
          <div className="flex gap-1">
            {[5, 10, 20].map((val) => (
              <button
                key={val}
                type="button"
                onClick={() =>
                  persistConfig({
                    ...config,
                    lowQuotaThresholdPercent: val,
                  })
                }
                className={`px-1.5 py-0.5 rounded text-[10px] font-semibold cursor-pointer ${
                  config.lowQuotaThresholdPercent === val
                    ? 'bg-indigo-500 text-white'
                    : 'bg-white/60 dark:bg-white/8 text-slate-500 dark:text-zinc-400 border border-black/8 dark:border-white/10'
                }`}
              >
                {val}%
              </button>
            ))}
          </div>
        </div>
      </section>

      {/* Custom Watch Paths */}
      <section className="rounded-xl bg-white/55 dark:bg-white/[0.06] border border-black/6 dark:border-white/[0.08] p-2.5 space-y-1.5">
        <div className="flex items-center justify-between">
          <span className="text-[11px] font-semibold text-slate-700 dark:text-zinc-200">
            감시 경로 커스텀 (비우면 기본값)
          </span>
          <button
            type="button"
            onClick={handleSavePaths}
            className="px-2 py-0.5 rounded bg-indigo-500 text-white text-[10px] font-semibold cursor-pointer hover:bg-indigo-600"
          >
            경로 저장
          </button>
        </div>

        <input
          type="text"
          value={codexPath}
          onChange={(e) => setCodexPath(e.target.value)}
          placeholder="Codex: ~/.codex/sessions"
          className="w-full px-2 py-1 text-[10px] rounded-md bg-white/80 dark:bg-black/25 border border-black/10 dark:border-white/12 text-slate-700 dark:text-zinc-200 placeholder:text-slate-400 dark:placeholder:text-zinc-500 focus:outline-none focus:border-indigo-400"
        />
        <input
          type="text"
          value={agyPath}
          onChange={(e) => setAgyPath(e.target.value)}
          placeholder="Antigravity: ~/.gemini/antigravity-cli"
          className="w-full px-2 py-1 text-[10px] rounded-md bg-white/80 dark:bg-black/25 border border-black/10 dark:border-white/12 text-slate-700 dark:text-zinc-200 placeholder:text-slate-400 dark:placeholder:text-zinc-500 focus:outline-none focus:border-indigo-400"
        />
        <input
          type="text"
          value={claudePath}
          onChange={(e) => setClaudePath(e.target.value)}
          placeholder="Claude: ~/.claude"
          className="w-full px-2 py-1 text-[10px] rounded-md bg-white/80 dark:bg-black/25 border border-black/10 dark:border-white/12 text-slate-700 dark:text-zinc-200 placeholder:text-slate-400 dark:placeholder:text-zinc-500 focus:outline-none focus:border-indigo-400"
        />
      </section>
    </div>
  );
}
