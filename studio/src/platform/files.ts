import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { log } from "./log";

export async function pickAndReadText(): Promise<{ name: string; text: string } | null> {
  const picked = await open({ multiple: false, directory: false });
  if (!picked || Array.isArray(picked)) return null;
  const text = await invoke<string>("read_text_file", { path: picked });
  const name = picked.replace(/^.*[\\/]/, "");
  log("info", `imported text file ${name}`);
  return { name, text };
}

export async function pickExportZipPath(): Promise<string | null> {
  const picked = await open({
    multiple: false,
    directory: false,
    filters: [{ name: "Letterboxd export", extensions: ["zip"] }],
  });
  if (!picked || Array.isArray(picked)) return null;
  return picked;
}
