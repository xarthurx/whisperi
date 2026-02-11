/**
 * Generate Whisperi app icons from SVG.
 *
 * Usage:  bun run scripts/generate-icons.mjs
 * Deps:   bun add -D sharp png-to-ico
 */

import sharp from "sharp";
import pngToIco from "png-to-ico";
import { writeFile, mkdir } from "fs/promises";
import { join, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ICONS_DIR = join(__dirname, "..", "src-tauri", "icons");

// SVG icon design — Nord palette, Apple-style squircle, microphone + sound waves
// Apple squircle path approximated as a continuous-curvature superellipse
const SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
  <defs>
    <!-- Nord Polar Night gradient background -->
    <linearGradient id="bg" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#3B4252"/>
      <stop offset="100%" stop-color="#2E3440"/>
    </linearGradient>
    <!-- Subtle Frost glow from top-left -->
    <radialGradient id="glow" cx="30%" cy="25%" r="70%">
      <stop offset="0%" stop-color="#88C0D0" stop-opacity="0.15"/>
      <stop offset="100%" stop-color="#2E3440" stop-opacity="0"/>
    </radialGradient>
    <!-- Microphone body gradient — Snow Storm -->
    <linearGradient id="mic" x1="50%" y1="0%" x2="50%" y2="100%">
      <stop offset="0%" stop-color="#ECEFF4"/>
      <stop offset="100%" stop-color="#D8DEE9"/>
    </linearGradient>
    <!-- Frost accent for sound waves -->
    <linearGradient id="wave" x1="50%" y1="0%" x2="50%" y2="100%">
      <stop offset="0%" stop-color="#88C0D0"/>
      <stop offset="100%" stop-color="#81A1C1"/>
    </linearGradient>
  </defs>

  <!-- Apple-style squircle (continuous curvature) -->
  <path d="
    M256 0
    C384 0, 426 0, 469 43
    C512 86, 512 128, 512 256
    C512 384, 512 426, 469 469
    C426 512, 384 512, 256 512
    C128 512, 86 512, 43 469
    C0 426, 0 384, 0 256
    C0 128, 0 86, 43 43
    C86 0, 128 0, 256 0Z
  " fill="url(#bg)"/>
  <path d="
    M256 0
    C384 0, 426 0, 469 43
    C512 86, 512 128, 512 256
    C512 384, 512 426, 469 469
    C426 512, 384 512, 256 512
    C128 512, 86 512, 43 469
    C0 426, 0 384, 0 256
    C0 128, 0 86, 43 43
    C86 0, 128 0, 256 0Z
  " fill="url(#glow)"/>

  <!-- Sound wave arcs (left) — Frost cyan -->
  <path d="M148 210 Q125 256 148 302" fill="none" stroke="#88C0D0" stroke-opacity="0.5" stroke-width="13" stroke-linecap="round"/>
  <path d="M115 175 Q82 256 115 337" fill="none" stroke="#88C0D0" stroke-opacity="0.25" stroke-width="13" stroke-linecap="round"/>

  <!-- Sound wave arcs (right) — Frost cyan -->
  <path d="M364 210 Q387 256 364 302" fill="none" stroke="#88C0D0" stroke-opacity="0.5" stroke-width="13" stroke-linecap="round"/>
  <path d="M397 175 Q430 256 397 337" fill="none" stroke="#88C0D0" stroke-opacity="0.25" stroke-width="13" stroke-linecap="round"/>

  <!-- Microphone body — Snow Storm white -->
  <rect x="222" y="140" width="68" height="150" rx="34" fill="url(#mic)"/>

  <!-- Microphone basket highlight -->
  <ellipse cx="256" cy="145" rx="34" ry="10" fill="#ECEFF4" fill-opacity="0.12"/>

  <!-- Microphone cradle (U shape) — Snow Storm -->
  <path d="M196 260 Q196 340 256 340 Q316 340 316 260" fill="none" stroke="#D8DEE9" stroke-width="15" stroke-linecap="round"/>

  <!-- Microphone stand -->
  <line x1="256" y1="340" x2="256" y2="388" stroke="#D8DEE9" stroke-width="15" stroke-linecap="round"/>

  <!-- Microphone base -->
  <line x1="218" y1="388" x2="294" y2="388" stroke="#D8DEE9" stroke-width="15" stroke-linecap="round"/>
</svg>`;

const SIZES = [
  { name: "32x32.png", size: 32 },
  { name: "128x128.png", size: 128 },
  { name: "128x128@2x.png", size: 256 },
  { name: "icon.png", size: 512 },
];

async function main() {
  await mkdir(ICONS_DIR, { recursive: true });

  const svgBuffer = Buffer.from(SVG);

  // Generate PNGs at each size
  for (const { name, size } of SIZES) {
    const png = await sharp(svgBuffer).resize(size, size).png().toBuffer();
    const outPath = join(ICONS_DIR, name);
    await writeFile(outPath, png);
    console.log(`  ✓ ${name} (${size}x${size})`);
  }

  // Generate ICO from the 256px PNG (includes 16, 32, 48, 256)
  const png256 = await sharp(svgBuffer).resize(256, 256).png().toBuffer();
  const icoBuffer = await pngToIco([png256]);
  await writeFile(join(ICONS_DIR, "icon.ico"), icoBuffer);
  console.log("  ✓ icon.ico");

  // Generate ICNS placeholder — on Windows we just copy the 512px PNG
  // macOS ICNS generation requires platform-specific tools
  // For CI/cross-platform builds, the PNGs + ICO are sufficient
  const png512 = await sharp(svgBuffer).resize(512, 512).png().toBuffer();
  await writeFile(join(ICONS_DIR, "icon.icns"), png512);
  console.log("  ✓ icon.icns (PNG fallback — use iconutil on macOS for real ICNS)");

  console.log("\nDone! Icons written to src-tauri/icons/");
}

main().catch((err) => {
  console.error("Icon generation failed:", err);
  process.exit(1);
});
