import { invoke } from "@tauri-apps/api/core";
import { log } from "./log";

export async function fetchText(url: string): Promise<string> {
  const text = await invoke<string>("fetch_text", { url });
  log("info", `fetched ${url} (${text.length} chars)`);
  return text;
}
