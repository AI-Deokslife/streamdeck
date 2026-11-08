// 배포용 설치 파일 만들기: 서명된 설치 파일 + 자동 업데이트용 latest.json
//
//   npm run release -- "이번 버전에서 바뀐 점"
//
// 서명 키는 저장소 밖(%USERPROFILE%\.tauri\quickdeck.key)에 둔다. 절대 커밋하지 말 것.
// 만들어진 파일 3개(설치 파일, .sig, latest.json)를 GitHub Releases 의 v<버전> 태그에 올리면
// 설치된 QuickDeck 들이 켤 때 새 버전을 알아챈다.
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REPO = "AI-Deokslife/streamdeck";
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const conf = JSON.parse(fs.readFileSync(path.join(root, "src-tauri", "tauri.conf.json"), "utf8"));
const { version, productName } = conf;

const keyPath = process.env.QD_SIGNING_KEY || path.join(os.homedir(), ".tauri", "quickdeck.key");
if (!fs.existsSync(keyPath)) {
  console.error(`서명 키가 없어요: ${keyPath}\n(npx tauri signer generate -w "${keyPath}" 로 만들 수 있지만, 그러면 이미 설치된 앱은 새 키로 서명된 업데이트를 받지 못해요)`);
  process.exit(1);
}

const env = {
  ...process.env,
  TAURI_SIGNING_PRIVATE_KEY: fs.readFileSync(keyPath, "utf8").trim(),
  // 키에 암호를 걸지 않았으면 빈 값 (PowerShell 에서는 빈 환경 변수를 만들 수 없어 여기서 넣는다)
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD: process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ?? "",
};
const build = spawnSync("npx", ["tauri", "build", "--bundles", "nsis"], { cwd: root, env, stdio: "inherit", shell: true });
if (build.status !== 0) process.exit(build.status ?? 1);

const dir = path.join(root, "src-tauri", "target", "release", "bundle", "nsis");
const exe = `${productName}_${version}_x64-setup.exe`;
const sigFile = path.join(dir, `${exe}.sig`);
if (!fs.existsSync(sigFile)) {
  console.error(`서명 파일이 없어요: ${sigFile}`);
  process.exit(1);
}
const latest = {
  version,
  notes: process.argv[2] || `QuickDeck v${version}`,
  pub_date: new Date().toISOString(),
  platforms: {
    "windows-x86_64": {
      signature: fs.readFileSync(sigFile, "utf8").trim(),
      url: `https://github.com/${REPO}/releases/download/v${version}/${exe}`,
    },
  },
};
fs.writeFileSync(path.join(dir, "latest.json"), JSON.stringify(latest, null, 2) + "\n");

console.log(`\n완료! GitHub Releases 에 태그 v${version} 로 아래 3개 파일을 올리세요:`);
for (const f of [exe, `${exe}.sig`, "latest.json"]) console.log("  " + path.join(dir, f));
