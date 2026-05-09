#!/usr/bin/env node
/**
 * 根据 commit 历史生成按类型分组的 release notes（markdown），输出到 stdout。
 *
 * 用法：
 *   node scripts/gen-release-notes.mjs <tag> [prev-tag]
 *
 * 前缀规则采用 conventional commits 风格：
 *   type(scope)?: subject
 * 比如 feat:、fix(ci):、build+ci:。release: 前缀视为版本 bump，不列入。
 *
 * 示例：
 *   node scripts/gen-release-notes.mjs v0.1.1
 *   node scripts/gen-release-notes.mjs v0.1.1 v0.1.0
 */

import { execSync } from "node:child_process";

const tag = process.argv[2];
if (!tag) {
  console.error("用法: node scripts/gen-release-notes.mjs <tag> [prev-tag]");
  process.exit(1);
}

let prevTag = process.argv[3];
if (!prevTag) {
  try {
    prevTag = execSync(`git describe --tags --abbrev=0 ${tag}^`, {
      encoding: "utf8",
    }).trim();
  } catch {
    prevTag = ""; // 首个 tag
  }
}

const range = prevTag ? `${prevTag}..${tag}` : tag;
const logRaw = execSync(`git log --pretty=format:%s%x00%h ${range}`, {
  encoding: "utf8",
});

const commits = logRaw
  .split("\n")
  .map((l) => l.trim())
  .filter(Boolean)
  .map((l) => {
    const [subject, hash] = l.split("\x00");
    return { subject, hash };
  });

// 分组定义：按顺序尝试匹配，第一个命中的组赢
const GROUPS = [
  { title: "✨ Features", match: /^feat(\+\w+)?(\([^)]+\))?:\s*/i },
  { title: "🐛 Bug Fixes", match: /^fix(\+\w+)?(\([^)]+\))?:\s*/i },
  { title: "🎨 UI", match: /^ui(\+\w+)?(\([^)]+\))?:\s*/i },
  { title: "📝 Documentation", match: /^docs(\+\w+)?(\([^)]+\))?:\s*/i },
  { title: "🛠 Build & CI", match: /^(build|ci|build\+ci|chore)(\([^)]+\))?:\s*/i },
  { title: "♻️ Refactor", match: /^refactor(\+\w+)?(\([^)]+\))?:\s*/i },
  { title: "🚀 Performance", match: /^perf(\+\w+)?(\([^)]+\))?:\s*/i },
  { title: "🧪 Tests", match: /^test(\+\w+)?(\([^)]+\))?:\s*/i },
];

// 整条过滤掉（不列入 release notes）
const EXCLUDE = [
  /^release(\([^)]+\))?:\s*/i, // 版本 bump，Tauri 的打 tag commit
  /^Merge\s/i, // 合并 commit 不列
];

const buckets = GROUPS.map((g) => ({ ...g, items: [] }));
const other = [];

for (const c of commits) {
  if (EXCLUDE.some((re) => re.test(c.subject))) continue;
  const hit = GROUPS.find((g) => g.match.test(c.subject));
  if (hit) {
    const cleaned = c.subject.replace(hit.match, "");
    buckets.find((b) => b.title === hit.title).items.push({ ...c, text: cleaned });
  } else {
    other.push(c);
  }
}

let md = "";
for (const b of buckets) {
  if (b.items.length === 0) continue;
  md += `### ${b.title}\n\n`;
  for (const c of b.items) md += `- ${c.text} (${c.hash})\n`;
  md += "\n";
}
if (other.length > 0) {
  md += `### 🔧 其他\n\n`;
  for (const c of other) md += `- ${c.subject} (${c.hash})\n`;
  md += "\n";
}

process.stdout.write(md.trimEnd());
