export type WindowLayout = {
  x: number;
  y: number;
  width: number;
  height: number;
  maximized: boolean;
};

export function nextWindowLayout(
  previous: WindowLayout | undefined,
  next: WindowLayout,
): WindowLayout {
  if (next.maximized) {
    return {
      x: previous?.x ?? next.x,
      y: previous?.y ?? next.y,
      width: previous?.width ?? next.width,
      height: previous?.height ?? next.height,
      maximized: true,
    };
  }
  return { ...next, maximized: false };
}
