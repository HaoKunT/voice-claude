#!/usr/bin/env node
/**
 * 根据 GitHub Release 的 assets 生成 Tauri updater 需要的 latest.json。
 *
 * 用法：
 *   node scripts/gen-latest-json.mjs <tag>
 *
 * 依赖环境：
 *   - 已安装 `gh` CLI 并 `gh auth login` 过（CI 里是 GITHUB_TOKEN）
 *   - GH_TOKEN 或 GITHUB_TOKEN 环境变量
 *
 * 产出：
 *   - 当前目录下 latest.json
 *   - 自动上传到同一个 Release（--clobber 覆盖旧的）
 */

import { execSync } from "node:child_process";
import { readFileSync, writeFileSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";

const tag = process.argv[2];
if (!tag) {
  console.error("用法: node scripts/gen-latest-json.mjs <tag>");
  process.exit(1);
}

const REPO = "HaoKunT/voice-claude";

function sh(cmd) {
  return execSync(cmd, { encoding: "utf8", stdio: ["pipe", "pipe", "inherit"] }).trim();
}

// 列出 release 的所有 asset
const assetsRaw = sh(`gh release view ${tag} -R ${REPO} --json assets,body,publishedAt`);
const { assets, body, publishedAt } = JSON.parse(assetsRaw);

// 平台匹配规则（Tauri v2）：
//   darwin-aarch64  ← *aarch64.app.tar.gz (+ .sig)
//   darwin-x86_64   ← *x64.app.tar.gz    (+ .sig)
//   windows-x86_64  ← *x64-setup.exe (NSIS) 优先；回退 *x64_en-US.msi
const PLATFORM_RULES = [
  { key: "darwin-aarch64", match: /aarch64\.app\.tar\.gz$/ },
  { key: "darwin-x86_64", match: /x64\.app\.tar\.gz$/ },
  { key: "windows-x86_64", match: /x64-setup\.exe$/ },
  { key: "windows-x86_64", match: /x64_en-US\.msi$/, fallback: true },
];

const tmp = mkdtempSync(path.join(tmpdir(), "updater-"));

function downloadSig(name) {
  const outPath = path.join(tmp, name);
  sh(`gh release download ${tag} -R ${REPO} -p '${name}' -D ${tmp} --clobber`);
  return readFileSync(outPath, "utf8").trim();
}

const platforms = {};
for (const rule of PLATFORM_RULES) {
  if (platforms[rule.key]) continue; // 同一个 key 前面的规则优先
  const bundle = assets.find((a) => rule.match.test(a.name));
  if (!bundle) continue;
  const sigAsset = assets.find((a) => a.name === `${bundle.name}.sig`);
  if (!sigAsset) {
    console.warn(`⚠ 找到 ${bundle.name} 但没有 .sig，跳过`);
    continue;
  }
  const signature = downloadSig(sigAsset.name);
  platforms[rule.key] = {
    signature,
    url: bundle.url,
  };
  console.log(`✓ ${rule.key}: ${bundle.name}`);
}

if (Object.keys(platforms).length === 0) {
  console.error("❌ 没匹配到任何平台 asset，检查 release 产物命名");
  process.exit(1);
}

const latest = {
  version: tag.replace(/^v/, ""),
  notes: body || "",
  pub_date: publishedAt || new Date().toISOString(),
  platforms,
};

const outFile = path.resolve("latest.json");
writeFileSync(outFile, JSON.stringify(latest, null, 2));
console.log(`✓ 已生成 ${outFile}`);

sh(`gh release upload ${tag} ${outFile} -R ${REPO} --clobber`);
console.log(`✓ 已上传到 release ${tag}`);
