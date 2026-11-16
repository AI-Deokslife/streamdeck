// 정보를 보여주는 버튼(시계·타이머·CPU/메모리)의 화면 표시
import { ActionType, DeckButton } from "./types";

export const WIDGET_TYPES: ActionType[] = ["clock", "timer", "sysInfo", "aiUsage"];

export interface TimerView {
  remainingMs: number;
  running: boolean;
  done: boolean;
}
/** Claude 사용량 창 하나 (resetsAt: 유닉스 초) */
export interface UsageWindow {
  pct: number;
  resetsAt: number;
}
export interface AiUsage {
  session: UsageWindow | null;
  week: UsageWindow | null;
  updated: number;
}
export interface WidgetState {
  cpu: number;
  memory: number;
  timers: Record<string, TimerView>;
  ai?: AiUsage | null;
}

/** 초기화 시각이 지났으면 0% */
function livePct(w: UsageWindow | null | undefined, now: Date): number | null {
  if (!w) return null;
  return w.resetsAt && w.resetsAt * 1000 <= now.getTime() ? 0 : Math.round(w.pct);
}

/** 버튼 이름 자리에 들어가게 짧게: 하루 안이면 "02:50", 더 뒤면 "금 20:00" */
export function formatReset(sec: number, now: Date): string {
  const d = new Date(sec * 1000);
  const t = `${pad(d.getHours())}:${pad(d.getMinutes())}`;
  return sec * 1000 - now.getTime() < 20 * 3600 * 1000 ? t : `${"일월화수목금토"[d.getDay()]} ${t}`;
}

/** AI 사용량 버튼의 이름 자리 (이름을 비워 두면): 세션 초기화 시각, 오래된 숫자면 언제 기준인지 */
export function aiUsageLabel(w: WidgetState | null, now: Date): string {
  const ai = w?.ai;
  if (!ai) return "Claude 연결 필요";
  const ago = Math.max(0, Math.round((now.getTime() / 1000 - ai.updated) / 60));
  const s = ai.session;
  const reset = s && s.resetsAt * 1000 > now.getTime() ? `리셋 ${formatReset(s.resetsAt, now)}` : "세션 리셋됨";
  return ago >= 60 ? `${Math.floor(ago / 60)}시간 전 기록` : reset;
}

/** 버튼 안에 글자로 보여줄 내용. progress(0~1)가 있으면 아래에 막대 */
export interface WidgetFace {
  lines: string[];
  progress?: number;
  state?: "running" | "paused" | "done";
}

const pad = (n: number) => String(n).padStart(2, "0");

export function formatDate(d: Date): string {
  return `${d.getMonth() + 1}월 ${d.getDate()}일 (${"일월화수목금토"[d.getDay()]})`;
}

export function formatRemaining(ms: number): string {
  const total = Math.ceil(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  return h ? `${h}:${pad(m)}:${pad(total % 60)}` : `${pad(m)}:${pad(total % 60)}`;
}

export function widgetFace(b: DeckButton, now: Date, w: WidgetState | null): WidgetFace | undefined {
  const a = b.action;
  switch (a.type) {
    case "clock":
      return { lines: [`${pad(now.getHours())}:${pad(now.getMinutes())}`] };
    case "sysInfo":
      return { lines: [`CPU ${w?.cpu ?? 0}%`, `RAM ${w?.memory ?? 0}%`], progress: (w?.cpu ?? 0) / 100 };
    case "aiUsage": {
      if (!w?.ai) return { lines: ["Claude", "연결 필요"] };
      const s = livePct(w.ai.session, now), wk = livePct(w.ai.week, now);
      return { lines: [`세션 ${s ?? "–"}%`, `주간 ${wk ?? "–"}%`], progress: (s ?? 0) / 100 };
    }
    case "timer": {
      const total = a.minutes * 60_000;
      const t = w?.timers[b.id];
      const remaining = t ? t.remainingMs : total;
      return {
        lines: [t?.done ? "끝!" : formatRemaining(remaining)],
        progress: total ? remaining / total : 0,
        state: t?.done ? "done" : t?.running ? "running" : "paused",
      };
    }
    default:
      return undefined;
  }
}
