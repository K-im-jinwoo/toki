import type { AccentTheme, MascotState } from '../types/usage';

interface PastelTokiProps {
  state: MascotState;
  theme?: AccentTheme;
  size?: number;
}

const THEME_PALETTE: Record<
  AccentTheme,
  { aura: string; scarf: string; earInner: string }
> = {
  default: { aura: 'rgba(129, 140, 248, 0.22)', scarf: '#818CF8', earInner: '#FBCFE8' },
  codex: { aura: 'rgba(59, 130, 246, 0.20)', scarf: '#60A5FA', earInner: '#BFDBFE' },
  antigravity: { aura: 'rgba(139, 92, 246, 0.20)', scarf: '#A78BFA', earInner: '#DDD6FE' },
  claude: { aura: 'rgba(249, 115, 22, 0.20)', scarf: '#FB923C', earInner: '#FED7AA' },
};

const ANIM_CLASS: Record<MascotState, string> = {
  sleep: 'animate-toki-bob',
  idle: 'animate-toki-bob',
  run: 'animate-toki-hop',
  sprint: 'animate-toki-sprint',
  alert: 'animate-toki-alert',
  exhausted: '',
};

export function PastelToki({
  state,
  theme = 'default',
  size = 68,
}: PastelTokiProps) {
  const colors = THEME_PALETTE[theme];
  const isSleeping = state === 'sleep';
  const isExhausted = state === 'exhausted';
  const isAlert = state === 'alert';
  const isFast = state === 'run' || state === 'sprint';

  const leftEarRotate = isSleeping
    ? -48
    : isExhausted
      ? -62
      : isFast
        ? -24
        : -8;
  const rightEarRotate = isSleeping
    ? 48
    : isExhausted
      ? 62
      : isFast
        ? -10
        : 8;

  return (
    <div
      className={`relative inline-flex items-center justify-center ${ANIM_CLASS[state]}`}
      style={{ width: size, height: size }}
    >
      <svg
        viewBox="0 0 96 96"
        className="w-full h-full drop-shadow-sm"
        aria-label={`Toki mascot (${state})`}
      >
        {/* Soft pastel aura */}
        <circle cx="48" cy="52" r="38" fill={colors.aura} />

        {/* Left ear */}
        <g transform={`translate(36 30) rotate(${leftEarRotate})`}>
          <ellipse
            cx="0"
            cy="-14"
            rx="7.5"
            ry="17"
            fill="#FFFFFF"
            stroke="#CBD5E1"
            strokeWidth="2"
          />
          <ellipse cx="0" cy="-13" rx="3.8" ry="10.5" fill={colors.earInner} />
        </g>

        {/* Right ear */}
        <g transform={`translate(60 30) rotate(${rightEarRotate})`}>
          <ellipse
            cx="0"
            cy="-14"
            rx="7.5"
            ry="17"
            fill="#FFFFFF"
            stroke="#CBD5E1"
            strokeWidth="2"
          />
          <ellipse cx="0" cy="-13" rx="3.8" ry="10.5" fill={colors.earInner} />
        </g>

        {/* Chubby body */}
        <ellipse
          cx="48"
          cy="68"
          rx="20"
          ry="15"
          fill="#FFFFFF"
          stroke="#CBD5E1"
          strokeWidth="2"
        />

        {/* Pastel collar / scarf */}
        <path
          d="M32 58 Q48 64 64 58 Q62 65 48 66 Q34 65 32 58 Z"
          fill={colors.scarf}
        />

        {/* Head */}
        <ellipse
          cx="48"
          cy="45"
          rx="23"
          ry="19"
          fill="#FFFFFF"
          stroke="#CBD5E1"
          strokeWidth="2"
        />

        {/* Rosy cheeks */}
        <ellipse cx="33" cy="49" rx="4.5" ry="2.8" fill="#FDA4AF" opacity="0.65" />
        <ellipse cx="63" cy="49" rx="4.5" ry="2.8" fill="#FDA4AF" opacity="0.65" />

        {/* Eyes by state */}
        {isSleeping ? (
          <>
            <path
              d="M35 44 Q39 47 43 44"
              fill="none"
              stroke="#475569"
              strokeWidth="2.2"
              strokeLinecap="round"
            />
            <path
              d="M53 44 Q57 47 61 44"
              fill="none"
              stroke="#475569"
              strokeWidth="2.2"
              strokeLinecap="round"
            />
          </>
        ) : isExhausted ? (
          <>
            <path
              d="M36 41 L42 46 M42 41 L36 46"
              stroke="#64748B"
              strokeWidth="2.2"
              strokeLinecap="round"
            />
            <path
              d="M54 41 L60 46 M60 41 L54 46"
              stroke="#64748B"
              strokeWidth="2.2"
              strokeLinecap="round"
            />
          </>
        ) : (
          <>
            <circle cx="39" cy="43" r="3.2" fill="#334155" />
            <circle cx="40" cy="41.8" r="1.1" fill="#FFFFFF" />
            <circle cx="57" cy="43" r="3.2" fill="#334155" />
            <circle cx="58" cy="41.8" r="1.1" fill="#FFFFFF" />
          </>
        )}

        {/* Nose & mouth */}
        <ellipse cx="48" cy="47.5" rx="2.4" ry="1.7" fill="#F472B6" />
        {isAlert ? (
          <circle cx="48" cy="52" r="2.2" fill="#F43F5E" />
        ) : isExhausted ? (
          <path
            d="M44 53 Q48 50 52 53"
            fill="none"
            stroke="#64748B"
            strokeWidth="1.8"
            strokeLinecap="round"
          />
        ) : (
          <path
            d="M44 51 Q48 54 52 51"
            fill="none"
            stroke="#475569"
            strokeWidth="1.8"
            strokeLinecap="round"
          />
        )}

        {/* Tiny paws */}
        <ellipse
          cx="36"
          cy="73"
          rx="5"
          ry="3.6"
          fill="#FFFFFF"
          stroke="#CBD5E1"
          strokeWidth="1.8"
        />
        <ellipse
          cx="60"
          cy="73"
          rx="5"
          ry="3.6"
          fill="#FFFFFF"
          stroke="#CBD5E1"
          strokeWidth="1.8"
        />

        {/* State overlays */}
        {isSleeping && (
          <text
            x="69"
            y="26"
            fontSize="12"
            fontWeight="700"
            fill="#818CF8"
            fontFamily="ui-sans-serif, system-ui, sans-serif"
          >
            zZ
          </text>
        )}

        {isAlert && (
          <g transform="translate(71 14)">
            <circle cx="8" cy="8" r="9" fill="#F43F5E" />
            <rect x="6.8" y="3" width="2.4" height="6.2" rx="1.2" fill="#FFFFFF" />
            <circle cx="8" cy="12" r="1.3" fill="#FFFFFF" />
          </g>
        )}

        {state === 'sprint' && (
          <g stroke="#60A5FA" strokeWidth="2.2" strokeLinecap="round" opacity="0.8">
            <line x1="8" y1="42" x2="18" y2="42" />
            <line x1="5" y1="50" x2="15" y2="50" />
          </g>
        )}

        {isExhausted && (
          <path
            d="M74 26 C74 26 78 31 78 34 A4 4 0 1 1 70 34 C70 31 74 26 74 26 Z"
            fill="#7DD3FC"
          />
        )}
      </svg>
    </div>
  );
}
