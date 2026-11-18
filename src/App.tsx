import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { PastelToki } from './components/PastelToki';
import { SessionList } from './components/SessionList';
import { SettingsView } from './components/SettingsView';
import { ToolCard } from './components/ToolCard';
import type {
  DataMode,
  MascotState,
  MockScenario,
  UsageSnapshot,
} from './types/usage';

const SCENARIOS: { id: MockScenario; label: string }[] = [
  { id: 'auto', label: 'Auto' },
  { id: 'run', label: 'Run' },
  { id: 'sprint', label: 'Sprint' },
  { id: 'alert', label: 'Alert' },
  { id: 'exhausted', label: 'Low' },
  { id: 'idle', label: 'Idle' },
  { id: 'sleep', label: 'Sleep' },
];

const MASCOT_LABELS: Record<MascotState, string> = {
  sleep: '휴식 중 (10분+ 유휴)',
  idle: '대기 중',
  run: '에이전트 1개 실행 중',
  sprint: '멀티 에이전트 병렬 실행 중!',
  alert: '사용자 확인 대기 (!)',
  exhausted: '쿼터 부족 (5% 이하)',
};

export function App() {
  const isDev = import.meta.env.DEV;
  const [snapshot, setSnapshot] = useState<UsageSnapshot | null>(null);
  const [mode, setMode] = useState<DataMode>('live');
  const [scenario, setScenario] = useState<MockScenario>('auto');
  const [showSettings, setShowSettings] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    invoke<UsageSnapshot>('get_usage_snapshot')
      .then(setSnapshot)
      .catch(() => {});

    if (isDev) {
      invoke<DataMode>('get_data_mode')
        .then(setMode)
        .catch(() => {});

      invoke<MockScenario>('get_mock_scenario')
        .then(setScenario)
        .catch(() => {});
    }

    const unlistenPromise = listen<UsageSnapshot>('usage-snapshot', (event) => {
      setSnapshot(event.payload);
    });

    return () => {
      unlistenPromise.then((unlisten) => unlisten());
    };
  }, [isDev]);

  // Automatically resize popover window to fit visible cards & settings view
  useLayoutEffect(() => {
    const el = containerRef.current;
    if (!el) return;

    const syncHeight = () => {
      const measured = Math.ceil(el.getBoundingClientRect().height);
      if (measured > 0) {
        void invoke('resize_popover', { height: measured }).catch(() => {});
      }
    };

    syncHeight();
    const observer = new ResizeObserver(syncHeight);
    observer.observe(el);
    return () => observer.disconnect();
  }, [showSettings, snapshot?.tools.length, mode]);

  async function handleModeToggle(nextMode: DataMode) {
    setMode(nextMode);
    try {
      const updated = await invoke<UsageSnapshot>('set_data_mode', {
        mode: nextMode,
      });
      setSnapshot(updated);
    } catch {
      // Ignore outside Tauri runtime
    }
  }

  async function handleScenarioChange(next: MockScenario) {
    setMode('mock');
    setScenario(next);
    try {
      const updated = await invoke<UsageSnapshot>('set_mock_scenario', {
        scenario: next,
      });
      setSnapshot(updated);
    } catch {
      // Ignore outside Tauri runtime
    }
  }

  async function handleQuit() {
    try {
      await invoke('quit_app');
    } catch {
      window.close();
    }
  }

  const mascotState: MascotState = snapshot?.overallState ?? 'idle';
  const nowUnix = snapshot?.generatedAt ?? Math.floor(Date.now() / 1000);
  const tools = snapshot?.tools ?? [];

  return (
    <div
      ref={containerRef}
      className="w-full rounded-[14px] bg-white/55 dark:bg-zinc-900/45 border border-black/10 dark:border-white/12 p-3.5 text-slate-800 dark:text-zinc-100 flex flex-col gap-2.5"
    >
      {/* Top Summary Header */}
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2.5 min-w-0">
          <PastelToki state={mascotState} size={50} />
          <div className="min-w-0">
            <div className="flex items-center gap-1.5">
              <h1 className="text-sm font-bold tracking-tight text-slate-900 dark:text-white">
                Toki
              </h1>
              {isDev && (
                <button
                  type="button"
                  onClick={() =>
                    handleModeToggle(mode === 'live' ? 'mock' : 'live')
                  }
                  className={`px-1.5 py-0.5 text-[10px] font-semibold rounded-full border cursor-pointer transition-colors ${
                    mode === 'live'
                      ? 'bg-emerald-500/15 text-emerald-700 border-emerald-500/25 dark:text-emerald-300'
                      : 'bg-indigo-500/15 text-indigo-600 border-indigo-500/25 dark:text-indigo-300'
                  }`}
                  title="Live / Mock 모드 전환"
                >
                  {mode === 'live' ? 'Live' : 'Mock'}
                </button>
              )}
            </div>
            <p className="text-[11px] text-slate-500 dark:text-zinc-400 truncate mt-0.5">
              {showSettings ? '설정 및 훅 관리' : MASCOT_LABELS[mascotState]}
            </p>
          </div>
        </div>

        <div className="flex items-center gap-1 shrink-0">
          <button
            type="button"
            onClick={() => setShowSettings((v) => !v)}
            className={`text-[11px] font-medium px-2 py-1 rounded-lg transition-colors cursor-pointer ${
              showSettings
                ? 'bg-indigo-500/15 text-indigo-600 dark:text-indigo-300'
                : 'text-slate-500 dark:text-zinc-300 hover:bg-black/5 dark:hover:bg-white/10'
            }`}
          >
            {showSettings ? '대시보드' : '설정'}
          </button>
          <button
            type="button"
            onClick={handleQuit}
            className="text-[11px] font-medium text-slate-400 dark:text-zinc-400 hover:text-rose-500 dark:hover:text-rose-400 px-2 py-1 rounded-lg hover:bg-rose-500/10 transition-colors cursor-pointer"
          >
            종료
          </button>
        </div>
      </div>

      {/* Main Content: Dashboard or Settings */}
      {showSettings ? (
        <SettingsView onSnapshotUpdated={setSnapshot} />
      ) : (
        <>
          <div className="flex flex-col gap-2">
            {tools.length === 0 ? (
              <div className="rounded-xl bg-white/50 dark:bg-white/[0.05] border border-black/6 dark:border-white/[0.08] p-4 text-center">
                <p className="text-xs text-slate-500 dark:text-zinc-400 mb-2">
                  표시 중인 도구가 없습니다.
                </p>
                <button
                  type="button"
                  onClick={() => setShowSettings(true)}
                  className="px-2.5 py-1 rounded-lg bg-indigo-500 text-white text-[11px] font-semibold cursor-pointer"
                >
                  설정에서 도구 켜기
                </button>
              </div>
            ) : (
              tools.map((tool) => (
                <ToolCard key={tool.tool} snapshot={tool} nowUnix={nowUnix} />
              ))
            )}
          </div>

          {tools.length > 0 && <SessionList tools={tools} nowUnix={nowUnix} />}
        </>
      )}

      {/* Footer (dev-only Mock switcher) */}
      {isDev && (
        <div className="pt-2 border-t border-black/8 dark:border-white/10 flex items-center justify-between gap-1">
          {mode === 'live' ? (
            <>
              <span className="text-[10px] text-slate-400 dark:text-zinc-400">
                로컬 세션 · 훅 캐시 감시 중
              </span>
              <button
                type="button"
                onClick={() => handleModeToggle('mock')}
                className="text-[10px] font-semibold text-indigo-600 dark:text-indigo-300 hover:underline cursor-pointer"
              >
                Mock 테스트 →
              </button>
            </>
          ) : (
            <>
              <button
                type="button"
                onClick={() => handleModeToggle('live')}
                className="text-[10px] font-semibold text-emerald-600 dark:text-emerald-400 hover:underline shrink-0 cursor-pointer"
              >
                ← Live
              </button>
              <div className="flex items-center gap-1 flex-wrap justify-end">
                {SCENARIOS.map((item) => {
                  const active = scenario === item.id;
                  return (
                    <button
                      key={item.id}
                      type="button"
                      onClick={() => handleScenarioChange(item.id)}
                      className={`px-1.5 py-0.5 rounded text-[10px] font-semibold transition-colors cursor-pointer ${
                        active
                          ? 'bg-indigo-500 text-white'
                          : 'bg-black/5 dark:bg-white/10 text-slate-500 dark:text-zinc-300 hover:bg-black/10 dark:hover:bg-white/15'
                      }`}
                    >
                      {item.label}
                    </button>
                  );
                })}
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}

export default App;
