# QuickDeck

Windows용 데스크톱 스트림덱. 화면 위에 항상 떠 있는 버튼 패널과 전역 단축키로 프로그램 실행, 단축키 전송, 텍스트 입력 등을 할 수 있습니다.

- **기술 스택**: Tauri v2 (Rust) + React + TypeScript
- **설정 파일**: `%APPDATA%\com.deck.streamdeck\config.json`

## 기능

- 항상 위에 떠 있는 창. 버튼을 클릭해도 작업 중인 창의 포커스를 빼앗지 않음 (`WS_EX_NOACTIVATE`)
- 버튼 동작: 프로그램/파일 실행, 단축키 보내기, URL 열기, 텍스트 입력, PowerShell 명령
- 창 보이기/숨기기 전역 단축키, 버튼별 전역 단축키
- 편집 모드: 버튼 추가/삭제, 드래그로 순서 변경, 단축키 녹화
- 트레이 아이콘, Windows 시작 시 자동 실행, 창 위치 기억, 중복 실행 방지
- 시작할 때 단축키가 다른 프로그램과 겹치면 대체 키로 자동 변경하고 알림 표시

## 개발

```bash
npm install
npm run tauri dev
```

## 설치 파일 빌드

```bash
npm run tauri build
```

결과물 위치: `src-tauri/target/release/bundle/` (`msi/`, `nsis/`)

## 구조

```
src/                  React UI
  App.tsx             버튼 그리드, 편집 모드
  Editor.tsx          버튼/설정 편집 패널
  HotkeyInput.tsx     단축키 녹화 입력
src-tauri/src/
  lib.rs              앱 진입점, 트레이, 전역 단축키, 명령
  actions.rs          버튼 동작 실행 (키 입력은 enigo 사용)
  config.rs           설정 구조와 저장
  winutil.rs          Windows 포커스 처리
```

앱 아이콘을 바꾸려면 `app-icon.svg`를 수정한 뒤 `npx tauri icon app-icon.svg`를 실행하세요.
