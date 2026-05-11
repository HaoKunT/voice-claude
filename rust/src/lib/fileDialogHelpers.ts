// save/open 对话框 + 文件读写的前端样板抽取。
// 用在「配置导入/导出」「热词 CSV 导入/导出」这类场景。

import { save as saveDialog, open as openDialog } from "@tauri-apps/plugin-dialog";

export interface FileFilter {
  name: string;
  extensions: string[];
}

/**
 * 弹 save dialog 让用户选保存位置，然后把 content 写进去。
 * 用户取消时返回 false；成功 true。
 */
export async function saveTextToFile(
  content: string,
  defaultFilename: string,
  filters: FileFilter[],
): Promise<boolean> {
  const path = await saveDialog({ defaultPath: defaultFilename, filters });
  if (!path) return false;
  const { writeTextFile } = await import("@tauri-apps/plugin-fs");
  await writeTextFile(path, content);
  return true;
}

/**
 * 弹 open dialog 让用户选文件，返回文件文本内容。
 * 用户取消或选了空值时返回 null。
 */
export async function readTextFromFile(
  filters: FileFilter[],
): Promise<string | null> {
  const selected = await openDialog({ filters, multiple: false });
  if (!selected || typeof selected !== "string") return null;
  const { readTextFile } = await import("@tauri-apps/plugin-fs");
  return readTextFile(selected);
}
