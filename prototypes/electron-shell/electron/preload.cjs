const { contextBridge, ipcRenderer } = require("electron");

contextBridge.exposeInMainWorld("studio", {
  version: () => ipcRenderer.invoke("app:version"),
  getSetting: (key) => ipcRenderer.invoke("settings:get", key),
  setSetting: (key, value) => ipcRenderer.invoke("settings:set", key, value),
  openFile: () => ipcRenderer.invoke("file:open"),
  setOverlayTheme: (theme) => ipcRenderer.invoke("window:overlay", theme),
});
