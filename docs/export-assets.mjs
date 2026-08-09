// Rasterizes the sefy brand SVGs into PNGs and a multi-size .ico.
// Run from the docs package so it resolves the local sharp install:
//   pnpm export-assets
import sharp from "sharp";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// Relative to this file, so the script carries no machine-specific paths.
const ASSETS = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..", "assets");

// The S tile (filled hex, bold code) is what reads at icon sizes; the L tile is
// the one with room for detail.
const S = path.join(ASSETS, "logo-s.svg");
const L = path.join(ASSETS, "logo.svg");
const BANNER = path.join(ASSETS, "banner.svg");

const ICO_SIZES = [16, 24, 32, 48, 64, 128, 256];

async function png(src, size, out) {
  await sharp(src, { density: 384 }).resize(size, size).png().toFile(out);
}

// Minimal ICO container: header + directory entries + embedded PNG payloads.
function buildIco(pngBuffers, sizes) {
  const count = pngBuffers.length;
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0); // reserved
  header.writeUInt16LE(1, 2); // type: icon
  header.writeUInt16LE(count, 4);

  const entries = Buffer.alloc(16 * count);
  let offset = 6 + 16 * count;
  pngBuffers.forEach((buf, i) => {
    const size = sizes[i];
    const e = 16 * i;
    entries.writeUInt8(size >= 256 ? 0 : size, e + 0); // width (0 means 256)
    entries.writeUInt8(size >= 256 ? 0 : size, e + 1); // height
    entries.writeUInt8(0, e + 2); // palette
    entries.writeUInt8(0, e + 3); // reserved
    entries.writeUInt16LE(1, e + 4); // color planes
    entries.writeUInt16LE(32, e + 6); // bits per pixel
    entries.writeUInt32LE(buf.length, e + 8);
    entries.writeUInt32LE(offset, e + 12);
    offset += buf.length;
  });

  return Buffer.concat([header, entries, ...pngBuffers]);
}

const icoParts = [];
for (const size of ICO_SIZES) {
  icoParts.push(await sharp(S, { density: 384 }).resize(size, size).png().toBuffer());
}
fs.writeFileSync(path.join(ASSETS, "icon.ico"), buildIco(icoParts, ICO_SIZES));
console.log("wrote icon.ico");

// Favicon and the large mark.
await png(S, 32, path.join(ASSETS, "favicon-32.png"));
await png(S, 180, path.join(ASSETS, "apple-touch-icon.png"));
await png(L, 512, path.join(ASSETS, "logo-512.png"));
console.log("wrote pngs");

// The docs site serves these from its own public/ directory.
const PUBLIC = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "public");
fs.copyFileSync(path.join(ASSETS, "apple-touch-icon.png"), path.join(PUBLIC, "apple-touch-icon.png"));
console.log("copied apple-touch-icon.png into docs/public");

// GitHub social preview: 1280x640. Two adjustments to the banner: its plate
// spans the full 720px while the artwork ends around x=475 in SVG units (trim
// the empty tail, or the mark lands off-centre), and the rounded plate over an
// identical background leaves a visible seam (drop it, keep the inner rows).
const bannerWidth = 1600;
const scale = bannerWidth / 720; // SVG user units -> rendered pixels
const bannerHeight = Math.round(170 * scale);
const inset = Math.round(6 * scale); // clears the plate's rounded edge
const artworkEnd = Math.round(490 * scale); // past the tagline and the rule
const banner = await sharp(BANNER, { density: 384 })
  .resize({ width: bannerWidth })
  .extract({
    left: inset,
    top: inset,
    width: artworkEnd - inset,
    height: bannerHeight - 2 * inset,
  })
  .png()
  .toBuffer();

await sharp({
  create: { width: 1280, height: 640, channels: 4, background: "#1B2126" },
})
  .composite([{ input: await sharp(banner).resize({ width: 880 }).png().toBuffer(), gravity: "centre" }])
  .png()
  .toFile(path.join(ASSETS, "social-preview.png"));
console.log("wrote social-preview.png");
