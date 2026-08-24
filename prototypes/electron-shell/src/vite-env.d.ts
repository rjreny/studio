export {};

declare global {
  interface Window {
    studio: {
      version: () => Promise<string>;
      getSetting: (key: string) => Promise<unknown>;
      setSetting: (key: string, value: unknown) => Promise<void>;
      openFile: () => Promise<{ name: string; bytes: number } | null>;
      setOverlayTheme: (theme: "dark" | "light") => Promise<void>;
    };
  }
}
