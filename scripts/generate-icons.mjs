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

// SVG icon design — teal/emerald gradient with stylized microphone + sound waves
const SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
  <defs>
    <!-- Background gradient -->
    <linearGradient id="bg" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#0d9488"/>
      <stop offset="50%" stop-color="#0f766e"/>
      <stop offset="100%" stop-color="#115e59"/>
    </linearGradient>
    <!-- Subtle inner glow -->
    <radialGradient id="glow" cx="35%" cy="30%" r="65%">
      <stop offset="0%" stop-color="#2dd4bf" stop-opacity="0.25"/>
      <stop offset="100%" stop-color="#0d9488" stop-opacity="0"/>
    </radialGradient>
    <!-- Microphone body gradient -->
    <linearGradient id="mic" x1="50%" y1="0%" x2="50%" y2="100%">
      <stop offset="0%" stop-color="#ffffff"/>
      <stop offset="100%" stop-color="#e2e8f0"/>
    </linearGradient>
  </defs>

  <!-- Rounded square background -->
  <rect width="512" height="512" rx="108" ry="108" fill="url(#bg)"/>
  <rect width="512" height="512" rx="108" ry="108" fill="url(#glow)"/>

  <!-- Sound wave arcs (left) -->
  <path d="M140 210 Q120 256 140 302" fill="none" stroke="white" stroke-opacity="0.35" stroke-width="14" stroke-linecap="round"/>
  <path d="M108 180 Q78 256 108 332" fill="none" stroke="white" stroke-opacity="0.2" stroke-width="14" stroke-linecap="round"/>

  <!-- Sound wave arcs (right) -->
  <path d="M372 210 Q392 256 372 302" fill="none" stroke="white" stroke-opacity="0.35" stroke-width="14" stroke-linecap="round"/>
  <path d="M404 180 Q434 256 404 332" fill="none" stroke="white" stroke-opacity="0.2" stroke-width="14" stroke-linecap="round"/>

  <!-- Microphone body -->
  <rect x="222" y="140" width="68" height="150" rx="34" fill="url(#mic)"/>

  <!-- Microphone basket (top arc) -->
  <ellipse cx="256" cy="145" rx="34" ry="10" fill="white" fill-opacity="0.15"/>

  <!-- Microphone cradle (U shape) -->
  <path d="M196 260 Q196 340 256 340 Q316 340 316 260" fill="none" stroke="white" stroke-width="16" stroke-linecap="round"/>

  <!-- Microphone stand -->
  <line x1="256" y1="340" x2="256" y2="390" stroke="white" stroke-width="16" stroke-linecap="round"/>

  <!-- Microphone base -->
  <line x1="216" y1="390" x2="296" y2="390" stroke="white" stroke-width="16" stroke-linecap="round"/>
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
