// 정보를 보여주는 버튼(시계·타이머·CPU/메모리)의 화면 표시
import { ActionType, DeckButton } from "./types";

export const WIDGET_TYPES: ActionType[] = ["clock", "timer", "sysInfo"];

export interface TimerView {
  remainingMs: number;
  running: boolean;
  done: boolean;
}
export interface WidgetState {
  cpu: number;
  memory: number;
  timers: Record<string, TimerView>;
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
