import { getVersion } from "@tauri-apps/api/app";

export function appVersion(): Promise<string> {
  return getVersion();
}
