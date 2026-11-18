# Toki — 4-Phase Coding Agent Prompt Plan (`docs/PROMPTS.md`)

Use these 4 prompts sequentially with your coding agent (Antigravity, Codex, Claude Code, Cursor).  
Verify each phase compiles and passes checks before moving to the next phase.

> **Prerequisite before Phase 1:** Ensure Rust (`rustup`, `rustc`, `cargo`) and Node.js are installed on your machine (`rustc --version && node --version`).

---

## Phase 1 — App Skeleton, Placeholder Sprites & Tray Animation Loop

```text
AGENTS.md와 docs/SPEC.md를 먼저 읽고 규칙을 숙지한 뒤 1단계만 구현해 줘.

목표: Tauri v2(Rust + React/TypeScript + Tailwind) 앱을 초기화하고,
메뉴바/시스템 트레이 아이콘 애니메이션(아이콘만 표시, 텍스트 없음)과
클릭 시 열리는 프레임리스 팝오버 창을 구현한다.

세부 요구사항:
1. 트레이 아이콘 스프라이트가 필요하므로, Node 스크립트(scripts/gen_tray_sprites.mjs)로
   메뉴바에 뭉개지지 않는 미니멀 플랫 실루엣 토끼("Toki") PNG(44x44 Retina, template-ready 단색 실루엣)를
   상태별(sleep_1~2, idle_1~2, run_1~4, sprint_1~4, alert_1~2, exhausted_1)로 assets/sprites/에 생성한다.
   팝오버 내부에는 부드러운 파스텔 톤 치비/플랫 2D 토끼 SVG 컴포넌트를 배치해 상태별 표정·모션을 보여준다.
2. Rust animator.rs에서 시작 시 모든 PNG를 tauri::image::Image로 1회 프리로드하고,
   백그라운드 태스크에서 현재 MascotState에 맞는 간격으로 tray.set_icon(...)을 호출한다.
   (매 틱마다 파일을 읽거나 PNG를 재디코딩하지 말 것)
3. 트레이 클릭 시 프레임리스·투명/둥근 모서리 팝오버 창을 트레이 아이콘 근처에 토글하고,
   포커스를 잃으면 자동으로 숨긴다. 팝오버에는 임시 상태 전환 테스트 버튼(Sleep/Idle/Run/Sprint/Alert/Exhausted)을 넣어
   트레이 애니메이션 전환을 눈으로 검증할 수 있게 한다.
4. 완료 후 cargo check와 프론트엔드 빌드를 실행해 에러가 없음을 확인한다.
```

---

## Phase 2 — Unified Schema, State Machine & Popover Dashboard UI

```text
AGENTS.md와 docs/SPEC.md를 기준으로 2단계를 구현해 줘. (아직 실제 로컬 로그 파서는 연결하지 마.)

목표: src-tauri/src/schema.rs와 src/types/usage.ts의 단일 통합 스키마(UsageSnapshot)를 고정하고,
상태 머신(state.rs)과 팝오버 대시보드 UI를 Mock 데이터 기반으로 완성한다.

세부 요구사항:
1. schema.rs에 ToolId(codex, antigravity, claude), AgentStatus, QuotaWindow(remainingPercent, usedPercent, resetsAt, windowMinutes),
   SessionSummary, ToolSnapshot, UsageSnapshot을 정의한다.
2. state.rs에 3개 도구의 상태를 종합해 MascotState(alert > sprint > run > exhausted > idle > sleep)를 계산하는 순수 함수를 작성하고,
   단위 테스트(cargo test)로 모든 우선순위 경계 조건을 검증한다.
3. Mock Collector를 통해 1초마다 변화하는 가상 데이터(실행중/대기/유휴 전환, 5h·Week 잔여 % 변화, 리셋 카운트다운)를 생성하고,
   Tauri 이벤트로 팝오버에 push하며 동시에 animator.rs의 트레이 애니메이션 상태를 갱신한다.
4. React 팝오버 UI(ToolCard, QuotaBar, SessionList)를 구현한다:
   - 상단: 전체 요약 헤더 + 1단계 테스트 모드 토글(Mock 시나리오 전환 가능)
   - 중단: Codex / Antigravity / Claude Code 3개 카드 (미니 상태 캐릭터, 상태 배지, 5h/Week 잔여 % 게이지 + 리셋 남은 시간)
   - 하단: 현재 활성/최근 세션 목록
5. cargo test와 npm run build 통과를 확인한다.
```

---

## Phase 3 — Real Collectors (Codex + Antigravity + Claude Code) & Hook Cache

```text
AGENTS.md와 docs/SPEC.md를 기준으로 Mock을 실제 수집기(collectors/)로 교체해 줘.

세부 요구사항:
1. collectors/codex.rs:
   - ~/.codex/sessions/ 하위를 재귀 감시(notify) + 주기적 재스캔(자정 YYYY/MM/DD 전환 대응)한다.
   - rollout-*.jsonl에서 session_meta, turn 진행 상태, event_msg(payload.type == "token_count")의
     rate_limits.primary(5h) 및 secondary(10080m=1주) used_percent와 resets_at을 파싱한다.
   - remaining_percent = (100.0 - used_percent).clamp(0.0, 100.0)로 계산한다.
2. collectors/antigravity.rs:
   - ~/.gemini/antigravity-cli/conversation_summaries.db (및 존재 시 ~/.gemini/antigravity/conversation_summaries.db)를
     반드시 read-only(SQLITE_OPEN_READ_ONLY)로 열어 status(RUNNING/IDLE), not_fully_idle, title, last_modified_time을 읽는다.
   - ~/.toki/hooks/antigravity.json 캐시에서 5h/Week 쿼터를 방어적으로(Option) 파싱한다.
3. collectors/claude.rs:
   - ~/.claude/projects/ 최근 JSONL 수정 시각으로 활성 세션을 추적하고,
     ~/.toki/hooks/claude.json 캐시에서 rate_limits.five_hour / seven_day를 방어적으로 파싱한다.
4. 모든 파서에 대해 src-tauri/tests/fixtures/ 샘플 파일을 만들고 cargo test로 파싱 정확도·누락 필드 예외 처리를 검증한다.
5. 미설치 도구(예: ~/.claude 없음)나 훅 미설정 상태에서도 크래시 없이 "미설정/조회 불가" 상태로 안전하게 표시되게 한다.
```

---

## Phase 4 — 1-Click `statusLine` Hook Setup, Alerts, Custom Paths & Autostart

```text
AGENTS.md와 docs/SPEC.md를 기준으로 4단계(운영 편의 기능)를 구현해 줘.

세부 요구사항:
1. statusLine 훅 원클릭 설정(hooks.rs + resources/hooks/):
   - Claude Code(~/.claude/settings.json)와 Antigravity(~/.gemini/antigravity-cli/settings.json)에
     Toki statusLine 훅을 안전하게 추가/제거하는 Rust 커맨드를 만든다.
   - 기존 settings.json의 다른 설정을 절대 손상시키지 않고, 기존 statusLine.command가 이미 있으면
     체인 실행(stdin을 훅 스크립트에서 읽고 기존 커맨드로 그대로 전달)하며, Antigravity에는 stack_with_default: true를 유지한다.
   - 훅 스크립트는 stdin의 원본 JSON을 ~/.toki/hooks/<tool>.json에 원자적(임시파일 후 rename)으로 저장한다.
2. 알림(Notification):
   - 세션이 응답 대기(!) 상태가 되거나 5h/Week 잔여량이 임계치(기본 10%) 이하로 떨어질 때
     중복 스팸 없이 OS 네이티브 알림을 1회 발송한다.
3. 설정 화면(SettingsView.tsx):
   - 감시 경로 커스텀(Codex/Antigravity/Claude 경로 재지정)
   - Claude/Antigravity statusLine 훅 설치 상태 확인 및 원클릭 설치/해제 버튼
   - OS 부팅 시 자동 실행(tauri-plugin-autostart) 토글 및 알림 임계치 설정
4. 모든 단위 테스트(cargo test), 타입 체크, 프로덕션 빌드(cargo tauri build --debug 또는 검증 가능한 빌드)를 확인한다.
```
