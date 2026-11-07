// iOS 홈 화면 스타일 앱 아이콘 (둥근 사각형). 폴더는 iOS 폴더처럼 안의 버튼을 작게 미리 보여준다.
import { DeckButton } from "./types";

function MiniIcon({ b }: { b: DeckButton }) {
  return (
    <span className="mini-icon" style={b.image && b.imageFull ? undefined : { background: b.color }}>
      {b.image ? <img src={b.image} alt="" draggable={false} className={b.imageFull ? "full" : ""} /> : b.icon}
    </span>
  );
}

export default function AppIcon({ b }: { b: DeckButton }) {
  const a = b.action;
  const isFolder = a.type === "folder";

  // 이미지가 없는 폴더: 반투명 유리판 위에 안의 버튼 최대 9개
  if (isFolder && !b.image && a.buttons.length > 0) {
    return (
      <span className="app-icon glass folder-icon">
        <span className="folder-grid">
          {a.buttons.slice(0, 9).map((c) => (
            <MiniIcon key={c.id} b={c} />
          ))}
        </span>
        <Badges b={b} />
      </span>
    );
  }

  const full = !!b.image && !!b.imageFull;
  return (
    <span className={"app-icon" + (isFolder && !b.image ? " glass" : "")} style={full || (isFolder && !b.image) ? undefined : { background: b.color }}>
      {b.image ? (
        <img className={full ? "full" : "fit"} src={b.image} alt="" draggable={false} />
      ) : (
        <span className="glyph">{b.icon || (isFolder ? "📁" : "")}</span>
      )}
      <Badges b={b} />
    </span>
  );
}

/** 오른쪽 위 작은 표시: 폴더 / 멀티 액션 / 전역 단축키 */
function Badges({ b }: { b: DeckButton }) {
  const marks: string[] = [];
  if (b.action.type === "folder" && b.image) marks.push("▦");
  if (b.action.type === "multi") marks.push("⛓");
  if (b.hotkey) marks.push("⌘");
  if (!marks.length) return null;
  return <span className="icon-badge">{marks.join(" ")}</span>;
}
