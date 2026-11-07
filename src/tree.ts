// 폴더 트리 다루기. path = 루트에서부터 들어간 폴더 버튼 id 목록.
import { DeckButton } from "./types";

export type Path = string[];

function childrenOf(b: DeckButton): DeckButton[] | null {
  return b.action.type === "folder" ? b.action.buttons : null;
}

/** path 위치의 버튼 목록. 경로가 깨졌으면 null */
export function buttonsAt(root: DeckButton[], path: Path): DeckButton[] | null {
  let list = root;
  for (const id of path) {
    const folder = list.find((b) => b.id === id);
    const kids = folder && childrenOf(folder);
    if (!kids) return null;
    list = kids;
  }
  return list;
}

/** path 위치의 버튼 목록을 fn으로 바꾼 새 트리 (불변 업데이트) */
export function updateAt(
  root: DeckButton[],
  path: Path,
  fn: (list: DeckButton[]) => DeckButton[],
): DeckButton[] {
  if (path.length === 0) return fn(root);
  const [head, ...rest] = path;
  return root.map((b) =>
    b.id === head && b.action.type === "folder"
      ? { ...b, action: { ...b.action, buttons: updateAt(b.action.buttons, rest, fn) } }
      : b,
  );
}

/** 폴더 이름 목록 (경로 표시용) */
export function folderLabels(root: DeckButton[], path: Path): string[] {
  const labels: string[] = [];
  let list = root;
  for (const id of path) {
    const folder = list.find((b) => b.id === id);
    if (!folder) break;
    labels.push(folder.label);
    list = childrenOf(folder) ?? [];
  }
  return labels;
}

/** id 폴더로 들어가는 경로 (폴더 자신 포함). 없으면 null */
export function pathToFolder(root: DeckButton[], id: string, base: Path = []): Path | null {
  for (const b of root) {
    const kids = childrenOf(b);
    if (!kids) continue;
    const here = [...base, b.id];
    if (b.id === id) return here;
    const found = pathToFolder(kids, id, here);
    if (found) return found;
  }
  return null;
}

/** 폴더 안 버튼 수 (하위 폴더 포함 전체) */
export function countDeep(list: DeckButton[]): number {
  return list.reduce((n, b) => n + 1 + countDeep(childrenOf(b) ?? []), 0);
}

export interface FolderOption {
  path: Path;
  label: string;
}

/** "다른 폴더로 이동"에 쓸 목록. exclude 버튼 자신과 그 하위 폴더는 뺀다 */
export function folderOptions(root: DeckButton[], excludeId: string): FolderOption[] {
  const out: FolderOption[] = [{ path: [], label: "🏠 처음 화면" }];
  function walk(list: DeckButton[], base: Path, labels: string[]) {
    for (const b of list) {
      const kids = childrenOf(b);
      if (!kids || b.id === excludeId) continue;
      const path = [...base, b.id];
      const names = [...labels, b.label];
      out.push({ path, label: "📂 " + names.join(" › ") });
      walk(kids, path, names);
    }
  }
  walk(root, [], []);
  return out;
}

/** path 안에서 가장 가까운 "프로그램 연결 폴더"의 프로그램들 (키 입력 대상으로 물려받음).
 *  예: 웹 브라우저 폴더 → ["chrome.exe", "msedge.exe", "whale.exe"] 중 열려 있는 것으로 보낸다 */
export function inheritedApps(root: DeckButton[], path: Path): string[] {
  let list = root;
  let apps: string[] = [];
  for (const id of path) {
    const folder = list.find((b) => b.id === id);
    if (!folder || folder.action.type !== "folder") break;
    if (folder.action.apps.length) apps = folder.action.apps;
    list = folder.action.buttons;
  }
  return apps;
}

/** a가 b로 시작하는지 (b 폴더 또는 그 하위에 있는지) */
export function isWithin(a: Path, b: Path): boolean {
  return b.length <= a.length && b.every((id, i) => a[i] === id);
}
