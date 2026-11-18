# Toki 🐰

**Codex · Antigravity (`agy`) · Claude Code**의 실시간 실행 상태와 남은 쿼터(`5h` / `Week` / `Argon`)를 메뉴바·시스템 트레이에서 한눈에 보여주는 크로스플랫폼 데스크탑 컴패니언 앱입니다.  
[RunCat](https://kyome.io/runcat/) 스타일의 트레이 캐릭터 애니메이션과 에이전트 상태·쿼터 대시보드를 **Tauri v2 (Rust + React/TypeScript)**로 구현했습니다.

---

## 주요 기능

- **메뉴바 / 시스템 트레이 실루엣 애니메이션 (아이콘 전용)**
  - `Alert (!)` > `Sprint (2+ 동시 실행)` > `Run (1개 실행)` > `Exhausted (잔여 5% 이하)` > `Idle (10분 이내 활동)` > `Sleep (10분+ 유휴)` 우선순위에 따라 토끼 실루엣 프레임이 자동 전환됩니다.
  - macOS 템플릿 아이콘(`icon_as_template`)으로 라이트/다크 메뉴바에 자동 반전되며, 팝오버 창에는 macOS 네이티브 `Effect::Popover` 블러가 적용됩니다.
- **도구별 상태 & 남은 쿼터(%) 카드**
  - **Codex:** `~/.codex/sessions/` JSONL 로그에서 별도 설정 없이 턴 상태(`Running`/`Waiting`/`Idle`)와 `5h`, `Week` 잔여 `%` 및 리셋 카운트다운 자동 파싱
  - **Antigravity (`agy`):** `conversation_summaries.db`(읽기 전용)에서 실행/유휴 상태 추적 + `statusLine` 훅으로 `5h`, `Week`, `Argon` 잔여 `%` 수집
  - **Claude Code:** `~/.claude/projects/` 세션 추적 + `statusLine` 훅으로 `5h`, `Week` 잔여 `%` 수집
- **편의 기능**
  - **도구 표시/숨기기:** 사용하지 않는 도구는 카드·세션·마스코트 상태·알림 계산에서 모두 제외 가능 (카드 수에 따라 팝오버 높이 자동 조절)
  - **`statusLine` 훅 원클릭 설치/해제:** 기존 `settings.json` 키와 기존 `statusLine.command` 체인을 손상시키지 않고 안전하게 연결
  - **OS 네이티브 알림 & 부팅 시 자동 실행:** 응답 대기(`!`) 및 저잔여 임계치(`5% / 10% / 20%`) 도달 시 중복 없이 알림 발송
  - **Live / Mock 전환:** 로컬 파일 감시(`Live`)와 6가지 상태 미리보기(`Mock`) 즉시 전환 가능

---

## 아키텍처

```text
toki/
├── AGENTS.md                        # 에이전트 작업 규칙 및 스키마 계약
├── docs/
│   ├── SPEC.md                      # 상세 설계 명세서
│   └── PROMPTS.md                   # 4단계 구현 프롬프트 기록
├── scripts/
│   └── gen_tray_sprites.mjs         # 트레이 실루엣 PNG(56x44) 및 앱 아이콘 생성
├── assets/sprites/                  # 상태별 트레이 실루엣 프레임 15종
├── resources/hooks/                 # statusLine 훅 스크립트 (.sh / .ps1)
├── src-tauri/                       # Rust 백엔드 (수집기 → 상태 머신 → 트레이 애니메이션)
│   ├── tests/fixtures/              # 파서 단위 테스트용 JSONL / JSON 샘플
│   └── src/
│       ├── schema.rs                # 통합 UsageSnapshot 스키마
│       ├── state.rs                 # 마스코트 상태 머신
│       ├── animator.rs              # PNG 프리로드 및 트레이 프레임 루프
│       ├── collectors/              # codex / antigravity / claude 수집기
│       ├── hooks.rs                 # settings.json 안전 훅 설치기
│       ├── config.rs                # ~/.toki/config.json 영구 설정
│       ├── notifier.rs              # OS 알림 중복 방지기
│       └── lib.rs                   # 트레이·팝오버·IPC 커맨드
└── src/                             # React + TypeScript + Tailwind 팝오버 UI
```

---

## 시작하기

### 사전 요구사항
- **Node.js** (v18 이상 권장)
- **Rust 툴체인** (`rustup`, `rustc`, `cargo`)
- **Tauri v2 OS별 빌드 도구** (macOS: Xcode Command Line Tools)

### 개발 모드 실행

```bash
npm install
npm run tauri dev
```

### 테스트 및 프로덕션 빌드

```bash
# Rust 단위 테스트 (파서 · 상태 머신 · 훅 설치 · 스프라이트 검증)
cd src-tauri && cargo test && cd ..

# 프론트엔드 타입 체크 및 번들
npm run build

# 독립 데스크탑 앱(.app / .dmg 등) 빌드
npm run tauri build
```

---

## `statusLine` 훅 동작 방식

Codex는 세션 로그 파일에 `rate_limits`를 직접 남기지만, Antigravity(`agy`)와 Claude Code는 계정 잔여 `%`를 실행 중 `statusLine` 훅의 표준 입력(`stdin`)으로만 전달합니다.

Toki의 `설정 → statusLine 훅 연동`에서 **원클릭 설치**를 누르면:
1. `~/.toki/bin/toki-statusline-hook.sh` (Windows는 `.ps1`)를 배치합니다.
2. 각 도구의 `settings.json`에 훅을 등록하며, 기존 `statusLine.command`가 있으면 파이프로 그대로 이어줍니다. (Antigravity는 `"stack_with_default": true` 유지)
3. 각 CLI가 응답할 때 들어온 JSON을 `~/.toki/hooks/<tool>.json`에 원자적으로 캐싱해 Toki가 즉시 읽어옵니다.
