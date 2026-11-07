// 버튼 아이콘 자동 가져오기: 프로그램(.exe) 아이콘, 웹사이트 아이콘
import { invoke } from "@tauri-apps/api/core";
import { imageToDataUrl } from "./image";
import { Action, DeckButton } from "./types";

export type IconSource = { kind: "program"; target: string } | { kind: "site"; url: string };

/** 이 동작에서 아이콘을 꺼낼 수 있는 대상 (단축키·텍스트 버튼은 제외 — 폴더 안 버튼이 전부 같은 아이콘이 되므로) */
export function iconSource(a: Action): IconSource | null {
  switch (a.type) {
    case "launch":
      return a.target.trim() ? { kind: "program", target: a.target.trim() } : null;
    case "folder": {
      const t = (a.launch || a.apps[0] || "").trim();
      return t ? { kind: "program", target: t } : null;
    }
    case "url": {
      const u = a.url.trim();
      return /^https?:\/\/[^/\s]+\.[^/\s]+/i.test(u) || /^[^/\s]+\.[a-z]{2,}(\/|$)/i.test(u) ? { kind: "site", url: u } : null;
    }
    default:
      return null;
  }
}

export function sourceKey(s: IconSource | null): string {
  return s ? (s.kind === "program" ? `p:${s.target.toLowerCase()}` : `s:${s.url}`) : "";
}

/** 자동 아이콘으로 바꿔도 되는 버튼인가 (직접 넣은 이미지는 건드리지 않음) */
export function canAutoIcon(b: DeckButton): boolean {
  return !b.image || !!b.imageAuto;
}

export interface IconPatch {
  image: string;
  imageFull: boolean;
  imageAuto: true;
}

/** 대상에서 아이콘을 가져와 버튼에 넣을 값으로. 실패하면 오류 메시지를 던진다 */
export async function fetchIcon(source: IconSource): Promise<IconPatch> {
  if (source.kind === "program") {
    const image = await invoke<string>("get_app_icon", { target: source.target });
    return { image, imageFull: false, imageAuto: true };
  }
  const icon = await invoke<{ dataUrl: string; full: boolean }>("get_site_icon", { url: source.url });
  // 사이트 아이콘은 형식(ico/svg/png)과 크기가 제각각이라 PNG 로 맞춘다
  const blob = await (await fetch(icon.dataUrl)).blob();
  return { image: await imageToDataUrl(blob), imageFull: icon.full, imageAuto: true };
}

/** 폴더 안까지 모든 버튼 */
export function allButtons(list: DeckButton[]): DeckButton[] {
  return list.flatMap((b) => [b, ...(b.action.type === "folder" ? allButtons(b.action.buttons) : [])]);
}
