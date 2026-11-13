import { useEffect, useRef } from "react";

// Mirror of the 128x64 SSD1306 frame buffer, served at /display.txt ('#' = lit pixel).
// The buffer is rendered by the firmware's own dashboard_core::display::render.

const COLS = 128;
const ROWS = 64;
const SCALE = 3;

export function Oled({ text }: { text: string | null }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const ctx = canvas.current?.getContext("2d");
    if (!ctx) return;
    ctx.fillStyle = "#05070a";
    ctx.fillRect(0, 0, COLS * SCALE, ROWS * SCALE);
    if (!text) return;
    ctx.fillStyle = "#7fd4ff";
    text
      .split("\n")
      .slice(0, ROWS)
      .forEach((line, y) => {
        for (let x = 0; x < Math.min(line.length, COLS); x++) {
          if (line[x] === "#") ctx.fillRect(x * SCALE, y * SCALE, SCALE, SCALE);
        }
      });
  }, [text]);
  return (
    <canvas
      ref={canvas}
      className="oled"
      width={COLS * SCALE}
      height={ROWS * SCALE}
      aria-label="OLED display mirror"
    />
  );
}
