import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import AppIcon from "./AppIcon";
import { ButtonEditor, SettingsEditor } from "./Editor";
import { prettyHotkey } from "./HotkeyInput";
import { firstImage, imageToDataUrl } from "./image";
import { IconPatch, allButtons, canAutoIcon, fetchIcon, iconSource } from "./icons";
import { AppPreset, presetFolder } from "./presets";
import {
  Path,
  buttonsAt,
  cloneWithNewIds,
  countDeep,
  folderLabels,
  folderOptions,
  inheritedApps,
  isWithin,
  pathToFolder,
  updateAt,
  updateById,
} from "./tree";
import { AppFocus, Config, DeckButton, STYLE_THEMES, newButton } from "./types";
import { WIDGET_TYPES, WidgetState, formatDate, widgetFace } from "./widgets";
import "./App.css";

/** 아이콘 아래 이름 줄 + 간격 */
const LABEL_SPACE = 23;
const MIN_ICON = 40;
const MAX_ICON = 120;

export default function App() {
  const [config, setConfig] = useState<Config | null>(null);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState<Config | null>(null);
  /** 지금 열려 있는 폴더 경로 (빈 배열 = 처음 화면) */
  const [path, setPath] = useState<Path>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [pressedId, setPressedId] = useState<string | null>(null);
  const [toast, setToast] = useState<{ text: string; kind: "error" | "info" } | null>(null);
  const dragId = useRef<string | null>(null);
  /** 편집 중 복사해 둔 버튼 (다른 폴더에 붙여넣기용) */
  const [copied, setCopied] = useState<DeckButton | null>(null);
  /** 토글 버튼 상태 (버튼 id → 켜짐) */
  const [toggles, setToggles] = useState<Record<string, boolean>>({});
  /** 길게 누르기: 타이머와, 길게 누르기가 실행돼서 이어지는 클릭을 무시해야 하는지 */
  const longTimer = useRef<number>(undefined);
  const longFired = useRef(false);
  useEffect(() => {
    invoke<Record<string, boolean>>("get_toggles").then(setToggles).catch(() => {});
    const un = listen<{ id: string; on: boolean }>("toggle-changed", (e) =>
      setToggles((t) => ({ ...t, [e.payload.id]: e.payload.on })),
    );
    return () => {
      un.then((f) => f());
    };
  }, []);
  /** 새 버전 알림과 내려받는 중 진행률 */
  const [update, setUpdate] = useState<{ version: string; current: string; notes: string; headline: string } | null>(null);
  const [updating, setUpdating] = useState<{ got: number; total: number | null } | null>(null);
  /** 위젯 버튼 값 (CPU·메모리, 타이머)과 시계용 현재 시각 */
  const [widgets, setWidgets] = useState<WidgetState | null>(null);
  const [now, setNow] = useState(() => new Date());
  const toastTimer = useRef<number>(undefined);
  /** 연결 프로그램 때문에 자동으로 연 폴더 (스마트 프로필) */
  const [autoApp, setAutoApp] = useState<string | null>(null);
  const auto = useRef<{ folderPath: Path; returnPath: Path } | null>(null);
  // 이벤트 핸들러에서 최신 값을 읽기 위한 ref
  const pathRef = useRef<Path>([]);
  const configRef = useRef<Config | null>(null);
  const editingRef = useRef(false);
  pathRef.current = path;
  configRef.current = config;
  editingRef.current = editing;

  /** 앞에 온 프로그램에 맞춰 화면 전환. 편집 중에는 쉰다 (Stream Deck과 같음) */
  const applyFocus = useCallback((ev: AppFocus) => {
    const cfg = configRef.current;
    if (!cfg || editingRef.current) return;
    const target = ev.folderId ? pathToFolder(cfg.buttons, ev.folderId) : null;
    const now = pathRef.current;

    if (target) {
      if (auto.current && isWithin(now, auto.current.folderPath) && auto.current.folderPath.join("/") === target.join("/")) {
        return; // 이미 그 폴더(또는 하위)에 있음
      }
      const returnPath = auto.current ? auto.current.returnPath : now;
      auto.current = { folderPath: target, returnPath };
      setAutoApp(ev.exe);
      if (!isWithin(now, target)) {
        setPath(target);
        setSelectedId(null);
      }
    } else if (auto.current) {
      // 연결 없는 프로그램으로 갔으면, 사용자가 그 폴더 안에 머물러 있던 경우에만 원래 화면으로
      if (isWithin(now, auto.current.folderPath)) setPath(auto.current.returnPath);
      auto.current = null;
      setAutoApp(null);
    }
  }, []);

  const syncCurrentApp = useCallback(() => {
    invoke<AppFocus>("current_app").then(applyFocus).catch(() => {});
  }, [applyFocus]);

  useEffect(() => {
    const un = listen<AppFocus>("app-focus", (e) => applyFocus(e.payload));
    return () => {
      un.then((f) => f());
    };
  }, [applyFocus]);

  const showToast = useCallback((text: string, kind: "error" | "info" = "error", ms = 4000) => {
    setToast({ text, kind });
    window.clearTimeout(toastTimer.current);
    toastTimer.current = window.setTimeout(() => setToast(null), ms);
  }, []);

  const startEdit = useCallback(async (cfg: Config) => {
    await invoke("set_edit_mode", { on: true });
    setDraft(structuredClone(cfg));
    setSelectedId(null);
    setEditing(true);
  }, []);

  useEffect(() => {
    invoke<Config>("get_config")
      .then((cfg) => {
        configRef.current = cfg;
        setConfig(cfg);
        syncCurrentApp();
      })
      .catch((e) => showToast(String(e)));
    // 단축키 등록 실패/자동 변경 같은 시작 알림은 오래 보여준다 (클릭하면 닫힘)
    invoke<string[]>("take_startup_notices").then((notices) => {
      if (notices.length) showToast(notices.join("\n"), "error", 12000);
    });
    const unErr = listen<string>("action-error", (e) => showToast(e.payload));
    return () => {
      unErr.then((f) => f());
    };
  }, [showToast, syncCurrentApp]);

  // 트레이 메뉴의 "버튼 편집"
  useEffect(() => {
    const un = listen("open-editor", () => {
      if (config && !editing) startEdit(config);
    });
    return () => {
      un.then((f) => f());
    };
  }, [config, editing, startEdit]);

  // 폴더 전역 단축키 → 그 폴더 열기
  useEffect(() => {
    const un = listen<string>("open-folder", (e) => {
      const tree = (editing && draft ? draft : config)?.buttons ?? [];
      const p = pathToFolder(tree, e.payload);
      if (p) {
        setPath(p);
        setSelectedId(null);
      }
    });
    return () => {
      un.then((f) => f());
    };
  }, [config, draft, editing]);

  const view = editing && draft ? draft : config;
  const current = view ? buttonsAt(view.buttons, path) : null;

  // 위젯 버튼이 하나라도 보이면 1초마다 값 갱신
  const showsWidgets = !!current?.some((b) => WIDGET_TYPES.includes(b.action.type));
  useEffect(() => {
    if (!showsWidgets) return;
    const tick = () => {
      setNow(new Date());
      invoke<WidgetState>("get_widgets").then(setWidgets).catch(() => {});
    };
    tick();
    const h = window.setInterval(tick, 1000);
    return () => window.clearInterval(h);
  }, [showsWidgets]);

  // 자동 업데이트 알림
  useEffect(() => {
    const uns = [
      listen<{ version: string; current: string; notes: string; headline: string }>("update-available", (e) => setUpdate(e.payload)),
      listen<{ text: string; kind: "info" | "error" }>("update-status", (e) => showToast(e.payload.text, e.payload.kind, 5000)),
      listen<{ got: number; total: number | null }>("update-progress", (e) => setUpdating(e.payload)),
    ];
    return () => {
      uns.forEach((u) => u.then((f) => f()));
    };
  }, [showToast]);

  async function installUpdate() {
    setUpdating({ got: 0, total: null });
    try {
      await invoke("install_update"); // 성공하면 설치 후 다시 시작해서 여기로 돌아오지 않음
    } catch (e) {
      setUpdating(null);
      showToast(String(e), "error", 8000);
    }
  }

  // 타이머가 끝나면 알림
  useEffect(() => {
    const un = listen<string>("timer-done", (e) => {
      const b = configRef.current && allButtons(configRef.current.buttons).find((x) => x.id === e.payload);
      showToast(`⏰ ${b?.label || "타이머"} 끝났어요!`, "info", 8000);
      invoke<WidgetState>("get_widgets").then(setWidgets).catch(() => {});
    });
    return () => {
      un.then((f) => f());
    };
  }, [showToast]);

  // 투명도: 마우스가 창 위에 있는지, 설정에서 슬라이더를 끄는 중인지(미리보기)
  const [hovered, setHovered] = useState(false);
  const [previewOpacity, setPreviewOpacity] = useState<number | null>(null);
  useEffect(() => {
    const root = document.documentElement;
    const enter = () => setHovered(true);
    const leave = () => setHovered(false);
    root.addEventListener("mouseenter", enter);
    root.addEventListener("mouseleave", leave);
    return () => {
      root.removeEventListener("mouseenter", enter);
      root.removeEventListener("mouseleave", leave);
    };
  }, []);

  // 테마: 편집 중에는 고르는 즉시 미리 보이게 draft 기준. 자동이면 Windows 설정이 바뀔 때도 따라간다
  const theme = view?.theme ?? "auto";
  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: light)");
    const apply = () => {
      if (STYLE_THEMES.includes(theme)) {
        document.documentElement.dataset.theme = theme;
        return;
      }
      const light = theme === "light" || (theme === "auto" && media.matches);
      document.documentElement.dataset.theme = light ? "light" : "dark";
    };
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [theme]);

  /** 배민 스타일 테마에서는 말투도 재치 있게 바꾼다: t(기본 문구, 배민 문구) */
  const fun = theme === "baemin";
  const t = (plain: string, funny: string) => (fun ? funny : plain);

  // 폴더가 삭제됐거나 편집 취소로 사라지면 처음 화면으로
  useEffect(() => {
    if (view && current === null) setPath([]);
  }, [view, current]);

  // 아이콘 크기: 창 너비만 보고 키우면 아래가 잘리므로, 모든 줄이 창 높이 안에 들어오는 크기로 맞춘다
  const gridRef = useRef<HTMLElement>(null);
  const [iconSize, setIconSize] = useState(60);

  // 편집 시트 높이: 손잡이를 위아래로 끌어 바꾼다 (null = 기본 크기). 이 PC 에 기억해 둔다
  const appRef = useRef<HTMLDivElement>(null);
  const [sheetH, setSheetH] = useState<number | null>(() => {
    try {
      const v = Number(localStorage.getItem("quickdeck.sheetH"));
      return v > 0 ? v : null;
    } catch {
      return null;
    }
  });
  const sheetDrag = useRef<{ startY: number; startH: number } | null>(null);
  function rememberSheet(h: number | null) {
    try {
      if (h) localStorage.setItem("quickdeck.sheetH", String(Math.round(h)));
      else localStorage.removeItem("quickdeck.sheetH");
    } catch {
      /* 저장 못 해도 지금 크기는 그대로 */
    }
  }
  function clampSheet(h: number) {
    const total = appRef.current?.clientHeight ?? window.innerHeight;
    // 위쪽 머리줄과 버튼 한 줄 정도는 남기고, 아래로는 손잡이와 몇 줄이 보일 만큼
    return Math.max(140, Math.min(total - 120, h));
  }
  function gripDown(e: React.PointerEvent<HTMLDivElement>) {
    const panel = e.currentTarget.parentElement;
    if (!panel) return;
    e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId);
    sheetDrag.current = { startY: e.clientY, startH: panel.getBoundingClientRect().height };
  }
  function gripMove(e: React.PointerEvent<HTMLDivElement>) {
    const d = sheetDrag.current;
    if (!d) return;
    setSheetH(clampSheet(d.startH + (d.startY - e.clientY)));
  }
  function gripUp() {
    if (!sheetDrag.current) return;
    sheetDrag.current = null;
    setSheetH((h) => {
      rememberSheet(h);
      return h;
    });
  }
  const tileCount =
    (current ?? view?.buttons ?? []).length + (path.length > 0 ? 1 : 0) + (editing ? 1 : 0) + (editing && copied ? 1 : 0);
  const columns = view?.columns ?? 4;
  useEffect(() => {
    const el = gridRef.current;
    if (!el) return;
    const measure = () => {
      const cs = getComputedStyle(el);
      const padX = parseFloat(cs.paddingLeft) + parseFloat(cs.paddingRight);
      const padY = parseFloat(cs.paddingTop) + parseFloat(cs.paddingBottom);
      const colGap = parseFloat(cs.columnGap) || 0;
      const rowGap = parseFloat(cs.rowGap) || 0;
      const colWidth = (el.clientWidth - padX - colGap * (columns - 1)) / columns;
      const byWidth = colWidth * 0.84;
      let size = byWidth;
      if (!editing) {
        // 편집 중이 아닐 때는 높이도 고려: 아이콘 + 이름(약 19px) 이 모든 줄에 들어가야 함
        const rows = Math.max(1, Math.ceil(tileCount / columns));
        const byHeight = (el.clientHeight - padY - rowGap * (rows - 1)) / rows - LABEL_SPACE;
        size = Math.min(byWidth, byHeight);
      }
      // 너무 작으면 글자가 안 보이므로 하한을 두고(그땐 스크롤), 너무 커도 어색하므로 상한
      setIconSize(Math.round(Math.max(MIN_ICON, Math.min(MAX_ICON, editing ? Math.min(size, 64) : size))));
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [tileCount, columns, editing]);

  // 편집 중 단축키 (Ctrl+C 복사, Ctrl+V 붙여넣기, Ctrl+D 복제). 핸들러는 매 렌더마다 최신으로 바꿔 끼운다
  const editKeys = useRef<(e: KeyboardEvent) => void>(undefined);
  useEffect(() => {
    const h = (e: KeyboardEvent) => editKeys.current?.(e);
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);

  // ---------- 편집 되돌리기 (Ctrl+Z / 다시 실행 Ctrl+Y) ----------
  const history = useRef<{ past: Config[]; future: Config[]; last: Config | null; lastAt: number; skip: boolean }>({
    past: [],
    future: [],
    last: null,
    lastAt: 0,
    skip: false,
  });
  useEffect(() => {
    const h = history.current;
    if (!editing || !draft) {
      Object.assign(h, { past: [], future: [], last: null, lastAt: 0, skip: false });
      return;
    }
    if (h.skip) {
      h.skip = false;
    } else if (h.last && h.last !== draft) {
      // 글자를 연달아 칠 때는 한 번으로 묶는다
      const now = Date.now();
      if (now - h.lastAt > 700 || !h.past.length) h.past.push(h.last);
      if (h.past.length > 100) h.past.shift();
      h.future = [];
      h.lastAt = now;
    }
    h.last = draft;
  }, [draft, editing]);

  if (!config || !view) return <div className="loading">불러오는 중…</div>;

  const buttons = current ?? view.buttons;
  const selected = editing ? buttons.find((b) => b.id === selectedId) ?? null : null;
  const crumbs = folderLabels(view.buttons, path);

  // 실제로 적용할 투명도: 편집 중엔 항상 선명(슬라이더 미리보기만 예외), 평소엔 설정값,
  // '마우스를 올리면 선명하게'가 켜져 있으면 마우스가 올라와 있을 때 100%
  const baseOpacity = Math.min(100, Math.max(20, view.opacity ?? 100)) / 100;
  const appOpacity =
    previewOpacity !== null
      ? previewOpacity / 100
      : editing || (view.clearOnHover && hovered)
        ? 1
        : baseOpacity;

  function undo(redo = false) {
    const h = history.current;
    const from = redo ? h.future : h.past;
    const to = redo ? h.past : h.future;
    const next = from.pop();
    if (!next || !draft) {
      showToast(redo ? "다시 실행할 게 없어요" : "더 되돌릴 게 없어요", "info", 1500);
      return;
    }
    to.push(draft);
    h.skip = true;
    h.lastAt = 0;
    setDraft(next);
    showToast(redo ? "다시 실행했어요" : "되돌렸어요 (다시 실행: Ctrl+Y)", "info", 1500);
  }

  editKeys.current = (e) => {
    if (!editing || !e.ctrlKey || e.altKey) return;
    // 글자 입력 칸이나 단축키 녹화 칸에서는 원래 동작 그대로
    const t = e.target as HTMLElement | null;
    if (t?.closest("input, textarea, select, [contenteditable], .keycap-box, .icon-preview")) return;
    const k = e.code;
    if (k === "KeyZ" || k === "KeyY") {
      e.preventDefault();
      undo(k === "KeyY" || e.shiftKey);
      return;
    }
    if (e.shiftKey) return;
    if (k === "KeyC" && selected) copyButton(selected);
    else if (k === "KeyV" && copied) pasteButton();
    else if (k === "KeyD" && selected) duplicateButton(selected);
    else return;
    e.preventDefault();
  };

  function openFolder(id: string) {
    setPath((p) => [...p, id]);
    setSelectedId(null);
  }

  function goTo(depth: number) {
    setPath((p) => p.slice(0, depth));
    setSelectedId(null);
  }

  /** 길게 누르기 시작 (편집 중이 아니고 길게 누르기 동작이 있을 때만) */
  function pressStart(b: DeckButton) {
    longFired.current = false;
    window.clearTimeout(longTimer.current);
    if (editing || (!b.longAction && b.action.type !== "timer")) return;
    longTimer.current = window.setTimeout(async () => {
      longFired.current = true;
      setPressedId(b.id);
      setTimeout(() => setPressedId(null), 250);
      try {
        await invoke("run_button", { id: b.id, long: true });
      } catch (e) {
        showToast(String(e));
      }
    }, 550);
  }
  function pressCancel() {
    window.clearTimeout(longTimer.current);
  }

  async function press(b: DeckButton) {
    if (longFired.current) {
      longFired.current = false; // 길게 누르기가 이미 실행됨
      return;
    }
    if (editing) {
      setSelectedId(b.id === selectedId ? null : b.id);
      return;
    }
    if (b.action.type === "folder") {
      const { apps, launch } = b.action;
      if (apps.length) {
        // 직접 연 폴더도 "연결 프로그램에서 벗어나면 원래 화면으로" 규칙을 따르게
        auto.current = { folderPath: [...path, b.id], returnPath: path };
        setAutoApp(b.label);
      }
      openFolder(b.id);
      // 연결 프로그램을 앞으로 가져오고, 실행 중이 아니면 실행 (경로가 비면 설치 위치를 자동으로 찾음)
      if (launch || apps.length) {
        invoke<"focused" | "launched" | "starting">("activate_app", { apps, launch })
          .then((r) => {
            if (r === "launched") showToast(t(`${b.label} 실행 중…`, `${b.label} 출동! 🚀`), "info", 2500);
            if (r === "starting") showToast(`${b.label}이(가) 아직 뜨는 중이에요. 잠시만 기다려 주세요`, "info", 2500);
          })
          .catch((e) => showToast(String(e), "error", 8000));
      }
      return;
    }
    setPressedId(b.id);
    setTimeout(() => setPressedId(null), 150);
    try {
      await invoke("run_button", { id: b.id });
    } catch (e) {
      showToast(String(e));
    }
  }

  async function finishEdit(save: boolean) {
    if (save && draft) {
      try {
        const warnings = await invoke<string[]>("save_config", { cfg: draft });
        setConfig(draft);
        configRef.current = draft;
        if (warnings.length) showToast(warnings.join("\n"));
        else showToast(t("저장했어요", "저장 완료! 오늘도 한 건 했네요 😎"), "info");
      } catch (e) {
        showToast(String(e));
        return;
      }
    }
    await invoke("set_edit_mode", { on: false });
    setEditing(false);
    setDraft(null);
    setSelectedId(null);
    // 편집하는 동안 쉬었던 자동 전환을 지금 앞에 있는 프로그램 기준으로 다시 맞춘다
    editingRef.current = false;
    auto.current = null;
    setAutoApp(null);
    syncCurrentApp();
  }

  async function addPreset(p: AppPreset) {
    // 설치 경로를 찾아서 "폴더를 누르면 실행"에 넣어 둔다
    let launch = "";
    for (const exe of p.apps) {
      const found = await invoke<string | null>("resolve_app_path", { exe }).catch(() => null);
      if (found) {
        launch = found;
        break;
      }
    }
    // 설치된 프로그램의 아이콘을 폴더 이미지로
    const image = launch ? await invoke<string>("get_app_icon", { target: launch }).catch(() => "") : "";
    const folder = presetFolder(p, launch, image);
    editHere((list) => [...list, folder]);
    setSelectedId(folder.id);
    // 연결 프로그램이 없는 프리셋(유튜브·Windows 기본)은 설치 위치를 찾을 필요가 없다
    const general = p.apps.length === 0;
    const base = general
      ? `'${p.name}' 폴더를 만들었어요.`
      : launch
        ? `'${p.name}' 폴더를 만들었어요. 누르면 ${p.name}이(가) 실행돼요.`
        : `'${p.name}' 폴더를 만들었어요. 설치 위치를 못 찾았으니 '폴더를 누르면 실행할 프로그램'에 경로를 넣어 주세요.`;
    showToast(p.note ? `${base}\n${p.note}` : base, general || launch ? "info" : "error", p.note ? 9000 : 5000);
  }

  /** 현재 폴더의 버튼 목록을 바꾼다 */
  function editHere(fn: (list: DeckButton[]) => DeckButton[]) {
    setDraft((d) => d && { ...d, buttons: updateAt(d.buttons, path, fn) });
  }

  function updateButton(b: DeckButton) {
    editHere((list) => list.map((x) => (x.id === b.id ? b : x)));
  }

  /** 비동기 작업(아이콘 가져오기) 뒤에 최신 편집 상태에 일부만 반영. 그새 사용자가 직접 이미지를 넣었으면 건드리지 않음 */
  function patchAutoIcon(id: string, patch: IconPatch) {
    setDraft((d) => d && { ...d, buttons: updateById(d.buttons, id, (b) => (canAutoIcon(b) ? { ...b, ...patch } : b)) });
  }

  /** 모든 버튼(폴더 안 포함)에 프로그램·사이트 아이콘 넣기. 직접 넣은 이미지는 그대로 둔다 */
  async function applyAllIcons() {
    if (!draft) return;
    const targets = allButtons(draft.buttons).filter((b) => canAutoIcon(b) && iconSource(b.action));
    if (!targets.length) {
      showToast("아이콘을 넣을 버튼이 없어요 (프로그램 실행·폴더·웹사이트 버튼만, 직접 넣은 이미지는 제외)", "info", 4000);
      return;
    }
    showToast(`아이콘 가져오는 중… (${targets.length}개)`, "info", 30000);
    let ok = 0;
    const queue = [...targets];
    // 사이트 아이콘은 네트워크라 몇 개씩 동시에
    await Promise.all(
      Array.from({ length: 4 }, async () => {
        for (let b = queue.shift(); b; b = queue.shift()) {
          const src = iconSource(b.action);
          if (!src) continue;
          try {
            patchAutoIcon(b.id, await fetchIcon(src));
            ok++;
          } catch {
            /* 실패한 버튼은 그대로 둔다 */
          }
        }
      }),
    );
    const fail = targets.length - ok;
    showToast(`아이콘 ${ok}개를 넣었어요${fail ? ` · ${fail}개는 찾지 못했어요` : ""}. 확인 후 '완료'를 누르세요`, "info", 5000);
  }

  function addButton() {
    const b = newButton();
    editHere((list) => [...list, b]);
    setSelectedId(b.id);
  }

  /** 바로 옆에 복사본 만들기 (폴더면 안의 버튼까지) */
  function duplicateButton(b: DeckButton) {
    const copy = cloneWithNewIds(b);
    copy.label = b.label ? `${b.label} 복사본` : "";
    editHere((list) => {
      const i = list.findIndex((x) => x.id === b.id);
      const next = [...list];
      next.splice(i + 1, 0, copy);
      return next;
    });
    setSelectedId(copy.id);
  }

  function copyButton(b: DeckButton) {
    setCopied(structuredClone(b));
    showToast(`'${b.label}'을(를) 복사했어요. 다른 폴더에서 '붙여넣기' 칸이나 Ctrl+V로 넣으세요`, "info", 3000);
  }

  function pasteButton() {
    if (!copied) return;
    const b = cloneWithNewIds(copied);
    editHere((list) => [...list, b]);
    setSelectedId(b.id);
  }

  function deleteButton(b: DeckButton) {
    if (b.action.type === "folder" && b.action.buttons.length > 0) {
      const n = countDeep(b.action.buttons);
      if (!window.confirm(`'${b.label}' 폴더와 안의 버튼 ${n}개를 모두 삭제할까요?`)) return;
    }
    editHere((list) => list.filter((x) => x.id !== b.id));
    setSelectedId(null);
  }

  function moveToFolder(b: DeckButton, to: Path) {
    if (to.join("/") === path.join("/")) return;
    setDraft((d) => {
      if (!d) return d;
      const removed = updateAt(d.buttons, path, (list) => list.filter((x) => x.id !== b.id));
      return { ...d, buttons: updateAt(removed, to, (list) => [...list, b]) };
    });
    setSelectedId(null);
    showToast(`'${b.label}'을(를) 옮겼어요`, "info", 2000);
  }

  function reorder(fromId: string, toId: string) {
    if (fromId === toId) return;
    editHere((list) => {
      const next = [...list];
      const from = next.findIndex((x) => x.id === fromId);
      const to = next.findIndex((x) => x.id === toId);
      if (from < 0 || to < 0) return list;
      const [item] = next.splice(from, 1);
      next.splice(to, 0, item);
      return next;
    });
  }

  return (
    <div
      ref={appRef}
      className={"app" + (editing ? " editing" : "") + (editing && sheetH ? " sheet-sized" : "")}
      style={{ opacity: appOpacity }}
    >
      {/* iOS 내비게이션 바: 왼쪽 뒤로/취소, 가운데 제목, 오른쪽 편집/완료 */}
      <header className="navbar" data-tauri-drag-region>
        <div className="nav-side left" data-tauri-drag-region>
          {editing ? (
            <button className="nav-btn" onClick={() => finishEdit(false)}>
              {t("취소", "안 할래요")}
            </button>
          ) : (
            path.length > 0 && (
              <button className="nav-back" onClick={() => goTo(path.length - 1)} title="상위 폴더로">
                <span className="chev">‹</span>
                {crumbs.length > 1 ? crumbs[crumbs.length - 2] : "홈"}
              </button>
            )
          )}
        </div>
        <div className="nav-title" data-tauri-drag-region>
          <span className="nav-title-text" data-tauri-drag-region>
            {path.length > 0 ? crumbs[crumbs.length - 1] : editing ? t("편집", "꾸미는 중") : "QuickDeck"}
          </span>
          <span className="nav-subtitle" data-tauri-drag-region>
            {editing ? (
              <span title="편집하는 동안은 프로그램별 자동 전환이 멈춰요">{t("자동 전환 일시정지", "자동 전환은 잠깐 쉬는 중")}</span>
            ) : autoApp ? (
              <span className="auto-pill" title={`${autoApp}에 연결된 폴더 — 다른 프로그램으로 가면 돌아가요`}>
                ● {autoApp.replace(/\.exe$/i, "")}
              </span>
            ) : path.length > 1 ? (
              <button className="nav-home" onClick={() => goTo(0)}>
                홈으로
              </button>
            ) : (
              t(prettyHotkey(config.toggleHotkey), `${prettyHotkey(config.toggleHotkey)} 누르면 짠!`)
            )}
          </span>
        </div>
        <div className="nav-side right" data-tauri-drag-region>
          {editing ? (
            <button className="nav-btn bold" onClick={() => finishEdit(true)}>
              {t("완료", "다 했어요")}
            </button>
          ) : (
            <>
              <button className="nav-btn" title="버튼 편집" onClick={() => startEdit(config)}>
                편집
              </button>
              <button className="nav-icon" title="숨기기 (트레이에 남아 있어요)" onClick={() => invoke("hide_window")}>
                <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true">
                  <path d="M2 6h8" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
                </svg>
              </button>
            </>
          )}
        </div>
      </header>

      <main
        ref={gridRef}
        className="grid"
        style={
          {
            gridTemplateColumns: `repeat(${view.columns}, 1fr)`,
            "--icon-size": `${iconSize}px`,
          } as React.CSSProperties
        }
      >
        {path.length > 0 && (
          <button
            className="tile back"
            onClick={() => goTo(path.length - 1)}
            title={editing ? "상위 폴더로 (버튼을 여기로 끌어 놓으면 상위 폴더로 옮겨져요)" : "상위 폴더로"}
            onDragOver={(e) => editing && e.preventDefault()}
            onDrop={() => {
              const b = buttons.find((x) => x.id === dragId.current);
              if (b) moveToFolder(b, path.slice(0, -1));
            }}
          >
            <span className="app-icon glass">
              <span className="glyph chevron">‹</span>
            </span>
            <span className="tile-label">{t("뒤로", "돌아가기")}</span>
          </button>
        )}
        {buttons.map((b) => {
          const isFolder = b.action.type === "folder";
          return (
            <button
              key={b.id}
              className={
                "tile" +
                (isFolder ? " folder" : "") +
                (pressedId === b.id ? " pressed" : "") +
                (selectedId === b.id ? " selected" : "")
              }
              onClick={() => press(b)}
              onPointerDown={() => pressStart(b)}
              onPointerUp={pressCancel}
              onPointerLeave={pressCancel}
              onContextMenu={(e) => b.longAction && e.preventDefault()}
              onDoubleClick={() => editing && isFolder && openFolder(b.id)}
              title={b.hotkey ? `${b.label} (${prettyHotkey(b.hotkey)})` : b.label}
              draggable={editing}
              onDragStart={() => (dragId.current = b.id)}
              onDragOver={(e) => editing && e.preventDefault()}
              onDrop={(e) => {
                if (!editing) return;
                // 탐색기에서 이미지 파일을 버튼 위에 끌어 놓으면 그 버튼 이미지로
                const file = firstImage(e.dataTransfer);
                if (file) {
                  e.preventDefault();
                  imageToDataUrl(file)
                    .then((image) => updateButton({ ...b, image, imageAuto: false }))
                    .catch((err) => showToast(String(err.message ?? err)));
                  return;
                }
                if (dragId.current) reorder(dragId.current, b.id);
              }}
            >
              <AppIcon b={b} on={!!toggles[b.id]} face={widgetFace(b, now, widgets)} />
              {(b.label || b.action.type === "clock") && (
                <span className="tile-label">
                  {b.action.type === "clock"
                    ? formatDate(now)
                    : toggles[b.id] && b.action.type === "toggle"
                      ? b.action.altLabel || b.label
                      : b.label}
                </span>
              )}
              {editing && (
                <span
                  className="del-badge"
                  role="button"
                  title="삭제"
                  onClick={(e) => {
                    e.stopPropagation();
                    deleteButton(b);
                  }}
                >
                  <svg viewBox="0 0 10 10" width="9" height="9" aria-hidden="true">
                    <path d="M2 5h6" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
                  </svg>
                </span>
              )}
            </button>
          );
        })}
        {editing && (
          <button className="tile add" onClick={addButton} title="이 화면에 버튼 추가">
            <span className="app-icon dashed">
              <span className="glyph plus">+</span>
            </span>
            <span className="tile-label">{t("추가", "하나 더!")}</span>
          </button>
        )}
        {editing && copied && (
          <button className="tile add" onClick={pasteButton} title={`'${copied.label}' 붙여넣기 (Ctrl+V)`}>
            <span className="app-icon dashed">
              <span className="glyph">📋</span>
            </span>
            <span className="tile-label">붙여넣기</span>
          </button>
        )}
      </main>

      {editing && draft && (
        <section className="panel" style={sheetH ? { flex: `0 0 ${clampSheet(sheetH)}px` } : undefined}>
          <div
            className="sheet-grip"
            role="separator"
            aria-orientation="horizontal"
            title="위아래로 끌어 크기 바꾸기 · 더블클릭하면 원래 크기"
            onPointerDown={gripDown}
            onPointerMove={gripMove}
            onPointerUp={gripUp}
            onPointerCancel={gripUp}
            onDoubleClick={() => {
              setSheetH(null);
              rememberSheet(null);
            }}
          />
          {selected ? (
            <ButtonEditor
              key={selected.id}
              button={selected}
              onChange={updateButton}
              onDelete={() => deleteButton(selected)}
              onDuplicate={() => duplicateButton(selected)}
              onCopy={() => copyButton(selected)}
              onTest={() =>
                invoke("test_action", {
                  action: selected.action,
                  inheritedApps: inheritedApps(draft.buttons, path),
                }).catch((e) => showToast(String(e)))
              }
              onOpenFolder={() => openFolder(selected.id)}
              folders={folderOptions(draft.buttons, selected.id)}
              currentPath={path}
              inherited={inheritedApps(draft.buttons, path)}
              onMove={(to) => moveToFolder(selected, to)}
              onAutoIcon={patchAutoIcon}
            />
          ) : (
            <SettingsEditor
              config={draft}
              onChange={setDraft}
              onAddPreset={addPreset}
              onNotice={(t, k) => showToast(t, k ?? "error", 5000)}
              onApplyAllIcons={applyAllIcons}
              onPreviewOpacity={setPreviewOpacity}
            />
          )}
        </section>
      )}

      {update && !editing && (
        <div className="update-bar">
          {updating ? (
            <>
              <span className="grow">
                내려받는 중…{" "}
                {updating.total ? `${Math.round((updating.got / updating.total) * 100)}%` : `${(updating.got / 1048576).toFixed(1)}MB`}
              </span>
            </>
          ) : (
            <>
              <span className="grow" title={update.notes || undefined}>
                🎉 새 버전 <b>v{update.version}</b>
                {update.headline && <small className="update-what">{update.headline}</small>}
              </span>
              <button className="update-go" onClick={installUpdate}>
                지금 업데이트
              </button>
              <button
                className="update-x"
                onClick={() => {
                  invoke("snooze_update", { version: update.version }).catch(() => {});
                  setUpdate(null);
                }}
                title="나중에 (하루 동안 다시 묻지 않아요)"
              >
                ✕
              </button>
            </>
          )}
        </div>
      )}

      {toast && (
        <div className={"toast " + toast.kind} onClick={() => setToast(null)} title="클릭해서 닫기">
          {toast.text}
        </div>
      )}
    </div>
  );
}
