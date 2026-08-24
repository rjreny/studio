import { LazyStore } from "@tauri-apps/plugin-store";
import { log } from "./log";

function createStore() {
  try {
    return new LazyStore("studio.json");
  } catch (err) {
    log("warn", "store unavailable", err);
    return null;
  }
}

const store = createStore();
const memory = new Map<string, unknown>();

export async function getSetting<T>(key: string): Promise<T | undefined> {
  if (!store) return memory.get(key) as T | undefined;
  try {
    return (await store.get<T>(key)) ?? undefined;
  } catch (err) {
    log("warn", `getSetting ${key} failed`, err);
    return memory.get(key) as T | undefined;
  }
}

export async function setSetting(key: string, value: unknown): Promise<void> {
  memory.set(key, value);
  if (!store) return;
  try {
    await store.set(key, value);
    await store.save();
  } catch (err) {
    log("warn", `setSetting ${key} failed`, err);
  }
}

export async function clearAllSettings(): Promise<void> {
  memory.clear();
  if (!store) return;
  try {
    await store.clear();
    await store.save();
  } catch (err) {
    log("warn", "clearAllSettings failed", err);
  }
}
