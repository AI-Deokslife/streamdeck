// 한 번에 배포하기:  npm run ship 0.7.1 "바뀐 점"
//
// 1) 버전 번호 3곳(package.json, Cargo.toml/Cargo.lock, tauri.conf.json)을 바꾸고
// 2) 커밋 + 태그 v<버전>(메시지 = 바뀐 점)을 만들어 GitHub 에 푸시한다.
// 3) 그러면 GitHub Actions(.github/workflows/release.yml)가 빌드·서명해서 릴리스를 게시하고,
//    설치된 QuickDeck 들은 다음에 켤 때 업데이트 알림을 띄운다.
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const [version, ...noteWords] = process.argv.slice(2);
const notes = noteWords.join(" ").trim();

const fail = (msg) => {
  console.error("✗ " + msg);
  process.exit(1);
};
const git = (...args) => execFileSync("git", args, { cwd: root, encoding: "utf8" }).trim();

if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) fail('사용법: npm run ship 0.7.1 "바뀐 점"');
const tag = `v${version}`;
if (git("tag", "-l", tag)) fail(`태그 ${tag} 가 이미 있어요. 버전 번호를 올려 주세요.`);
// 추적 중인 파일에 커밋 안 한 변경이 있으면 멈춘다 (새 파일은 상관없음)
if (git("status", "--porcelain", "--untracked-files=no")) fail("커밋하지 않은 변경이 있어요. 먼저 커밋하거나 되돌려 주세요.");

const edit = (rel, fn) => {
  const p = path.join(root, rel);
  const before = fs.readFileSync(p, "utf8");
  const after = fn(before);
  if (after === before) fail(`${rel} 에서 버전을 찾지 못했어요`);
  fs.writeFileSync(p, after);
};
const conf = JSON.parse(fs.readFileSync(path.join(root, "src-tauri/tauri.conf.json"), "utf8"));
if (conf.version === version) fail(`이미 ${version} 이에요. 더 높은 번호를 주세요.`);

edit("package.json", (s) => s.replace(/("version":\s*")[^"]+(")/, `$1${version}$2`));
edit("src-tauri/tauri.conf.json", (s) => s.replace(/("version":\s*")[^"]+(")/, `$1${version}$2`));
edit("src-tauri/Cargo.toml", (s) => s.replace(/^version = "[^"]+"/m, `version = "${version}"`));
edit("src-tauri/Cargo.lock", (s) => s.replace(/(name = "quickdeck"\r?\nversion = ")[^"]+(")/, `$1${version}$2`));

const message = notes ? `${tag}: ${notes}` : tag;
git("add", "package.json", "src-tauri/tauri.conf.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock");
git("commit", "-m", message);
git("tag", "-a", tag, "-m", notes || `QuickDeck ${tag}`);
console.log(`커밋·태그 ${tag} 완료. GitHub 에 올리는 중…`);
execFileSync("git", ["push", "origin", "HEAD"], { cwd: root, stdio: "inherit" });
execFileSync("git", ["push", "origin", tag], { cwd: root, stdio: "inherit" });
console.log(`
✓ ${tag} 를 올렸어요. GitHub 가 10분쯤 빌드한 뒤 릴리스를 자동으로 게시합니다.
  진행 상황: https://github.com/AI-Deokslife/streamdeck/actions`);
