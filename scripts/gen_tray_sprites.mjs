import fs from 'node:fs';
import path from 'node:path';
import zlib from 'node:zlib';

const ROOT = path.resolve(import.meta.dirname, '..');
const SPRITES_DIR = path.join(ROOT, 'assets', 'sprites');
const ICONS_DIR = path.join(ROOT, 'src-tauri', 'icons');

fs.mkdirSync(SPRITES_DIR, { recursive: true });
fs.mkdirSync(ICONS_DIR, { recursive: true });

// --- Minimal Zero-Dep RGBA PNG Encoder ---
const crcTable = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) {
      c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    }
    t[n] = c >>> 0;
  }
  return t;
})();

function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) {
    c = crcTable[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  }
  return (c ^ 0xffffffff) >>> 0;
}

function pngChunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const typeBuf = Buffer.from(type, 'ascii');
  const crcBuf = Buffer.alloc(4);
  crcBuf.writeUInt32BE(crc32(Buffer.concat([typeBuf, data])), 0);
  return Buffer.concat([len, typeBuf, data, crcBuf]);
}

function encodePng(width, height, rgba) {
  const raw = Buffer.alloc((width * 4 + 1) * height);
  for (let y = 0; y < height; y++) {
    const rowStart = y * (width * 4 + 1);
    raw[rowStart] = 0; // filter type 0
    rgba.copy(raw, rowStart + 1, y * width * 4, (y + 1) * width * 4);
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  ihdr[10] = 0;
  ihdr[11] = 0;
  ihdr[12] = 0;

  const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  return Buffer.concat([
    signature,
    pngChunk('IHDR', ihdr),
    pngChunk('IDAT', zlib.deflateSync(raw, { level: 9 })),
    pngChunk('IEND', Buffer.alloc(0)),
  ]);
}

// --- SDF Canvas Helper for Smooth Silhouettes ---
function createCanvas(w, h) {
  const buf = Buffer.alloc(w * h * 4, 0);

  function blendPixel(x, y, r, g, b, alpha) {
    if (x < 0 || x >= w || y < 0 || y >= h || alpha <= 0) return;
    const idx = (y * w + x) * 4;
    const srcA = Math.min(1, Math.max(0, alpha));
    const dstA = buf[idx + 3] / 255;
    const outA = srcA + dstA * (1 - srcA);
    if (outA <= 0) return;
    buf[idx] = Math.round((r * srcA + buf[idx] * dstA * (1 - srcA)) / outA);
    buf[idx + 1] = Math.round((g * srcA + buf[idx + 1] * dstA * (1 - srcA)) / outA);
    buf[idx + 2] = Math.round((b * srcA + buf[idx + 2] * dstA * (1 - srcA)) / outA);
    buf[idx + 3] = Math.round(outA * 255);
  }

  function fillRotatedEllipse(cx, cy, rx, ry, angleDeg, r = 0, g = 0, b = 0, a = 1) {
    const rad = (angleDeg * Math.PI) / 180;
    const cos = Math.cos(rad);
    const sin = Math.sin(rad);
    const pad = Math.ceil(Math.max(rx, ry)) + 2;
    const x0 = Math.max(0, Math.floor(cx - pad));
    const x1 = Math.min(w - 1, Math.ceil(cx + pad));
    const y0 = Math.max(0, Math.floor(cy - pad));
    const y1 = Math.min(h - 1, Math.ceil(cy + pad));
    for (let y = y0; y <= y1; y++) {
      for (let x = x0; x <= x1; x++) {
        const dx = x + 0.5 - cx;
        const dy = y + 0.5 - cy;
        const ux = dx * cos + dy * sin;
        const uy = -dx * sin + dy * cos;
        const dist = Math.sqrt((ux / rx) ** 2 + (uy / ry) ** 2);
        const pxDist = (dist - 1) * Math.min(rx, ry);
        const cov = Math.min(1, Math.max(0, 0.5 - pxDist));
        if (cov > 0) blendPixel(x, y, r, g, b, cov * a);
      }
    }
  }

  function fillCircle(cx, cy, radius, r = 0, g = 0, b = 0, a = 1) {
    fillRotatedEllipse(cx, cy, radius, radius, 0, r, g, b, a);
  }

  function fillCapsule(x1, y1, x2, y2, radius, r = 0, g = 0, b = 0, a = 1) {
    const minX = Math.max(0, Math.floor(Math.min(x1, x2) - radius - 2));
    const maxX = Math.min(w - 1, Math.ceil(Math.max(x1, x2) + radius + 2));
    const minY = Math.max(0, Math.floor(Math.min(y1, y2) - radius - 2));
    const maxY = Math.min(h - 1, Math.ceil(Math.max(y1, y2) + radius + 2));
    const vx = x2 - x1;
    const vy = y2 - y1;
    const lenSq = vx * vx + vy * vy || 1;
    for (let y = minY; y <= maxY; y++) {
      for (let x = minX; x <= maxX; x++) {
        const px = x + 0.5 - x1;
        const py = y + 0.5 - y1;
        const t = Math.min(1, Math.max(0, (px * vx + py * vy) / lenSq));
        const dx = px - vx * t;
        const dy = py - vy * t;
        const dist = Math.sqrt(dx * dx + dy * dy) - radius;
        const cov = Math.min(1, Math.max(0, 0.5 - dist));
        if (cov > 0) blendPixel(x, y, r, g, b, cov * a);
      }
    }
  }

  function cutoutCircle(cx, cy, radius) {
    const pad = Math.ceil(radius) + 2;
    for (let y = Math.max(0, Math.floor(cy - pad)); y <= Math.min(h - 1, Math.ceil(cy + pad)); y++) {
      for (let x = Math.max(0, Math.floor(cx - pad)); x <= Math.min(w - 1, Math.ceil(cx + pad)); x++) {
        const dist = Math.hypot(x + 0.5 - cx, y + 0.5 - cy) - radius;
        const erase = Math.min(1, Math.max(0, 0.5 - dist));
        if (erase > 0) {
          const idx = (y * w + x) * 4;
          buf[idx + 3] = Math.round(buf[idx + 3] * (1 - erase));
        }
      }
    }
  }

  function cutoutCapsule(x1, y1, x2, y2, radius) {
    const minX = Math.max(0, Math.floor(Math.min(x1, x2) - radius - 2));
    const maxX = Math.min(w - 1, Math.ceil(Math.max(x1, x2) + radius + 2));
    const minY = Math.max(0, Math.floor(Math.min(y1, y2) - radius - 2));
    const maxY = Math.min(h - 1, Math.ceil(Math.max(y1, y2) + radius + 2));
    const vx = x2 - x1;
    const vy = y2 - y1;
    const lenSq = vx * vx + vy * vy || 1;
    for (let y = minY; y <= maxY; y++) {
      for (let x = minX; x <= maxX; x++) {
        const px = x + 0.5 - x1;
        const py = y + 0.5 - y1;
        const t = Math.min(1, Math.max(0, (px * vx + py * vy) / lenSq));
        const dx = px - vx * t;
        const dy = py - vy * t;
        const dist = Math.sqrt(dx * dx + dy * dy) - radius;
        const erase = Math.min(1, Math.max(0, 0.5 - dist));
        if (erase > 0) {
          const idx = (y * w + x) * 4;
          buf[idx + 3] = Math.round(buf[idx + 3] * (1 - erase));
        }
      }
    }
  }

  return { buf, fillRotatedEllipse, fillCircle, fillCapsule, cutoutCircle, cutoutCapsule };
}

// Draw Toki silhouette rabbit (56x44, wide + full-height) facing right
function drawTrayRabbit(pose) {
  const c = createCanvas(56, 44);
  // Black silhouette (alpha channel used by macOS template tray icon)
  const k = [0, 0, 0];

  if (pose === 'sleep_1' || pose === 'sleep_2') {
    const breath = pose === 'sleep_2' ? -1.1 : 0;
    // Curled sleeping body (tucked neatly at bottom)
    c.fillRotatedEllipse(21, 34 + breath * 0.35, 11.2, 7.8 + breath * 0.3, -6, ...k);
    // Small tucked pom-pom tail (close to body, not sticking out like a fish fin)
    c.fillCircle(9.8, 36.2 + breath * 0.2, 3.2, ...k);
    // Resting front paw
    c.fillCapsule(28, 39.5, 36, 40.8, 2.3, ...k);
    // Head resting forward-down
    c.fillRotatedEllipse(33.5, 27.5 + breath * 0.45, 8.2, 7.1, 14, ...k);
    // Two distinct sleepy rabbit ears tilted softly back with clear separation above head
    c.fillRotatedEllipse(24.5, 16.0 + breath * 0.45, 3.0, 8.4, -36, ...k);
    c.fillRotatedEllipse(30.2, 14.5 + breath * 0.45, 2.9, 8.2, -18, ...k);
    // Peaceful closed eye cutout
    c.cutoutCapsule(32.5, 27.0 + breath * 0.45, 37.5, 27.8 + breath * 0.45, 1.15);
    // Crisp Z floating top-right (rises gently on sleep_2)
    const zy = pose === 'sleep_2' ? 6 : 9;
    c.fillCapsule(44, zy, 52, zy, 1.35, ...k);
    c.fillCapsule(52, zy, 44, zy + 8, 1.35, ...k);
    c.fillCapsule(44, zy + 8, 52, zy + 8, 1.35, ...k);
    return c.buf;
  }

  if (pose === 'idle_1' || pose === 'idle_2') {
    const bob = pose === 'idle_2' ? -1.4 : 0;
    const earTilt = pose === 'idle_2' ? -6 : -16;
    // Sitting body (fills y ~ 20..42)
    c.fillRotatedEllipse(23, 33 + bob * 0.4, 12.0, 9.8, -16, ...k);
    // Fluffy tail
    c.fillCircle(10.5, 35, 4.4, ...k);
    // Paws grounded at y ~ 40
    c.fillCapsule(31, 38, 37, 40.5, 2.7, ...k);
    c.fillCapsule(19, 40.5, 29, 41.0, 2.8, ...k);
    // Head
    c.fillRotatedEllipse(34, 23 + bob, 8.6, 7.4, 8, ...k);
    // Tall upright ears reaching y ~ 2
    c.fillRotatedEllipse(29.5, 10.5 + bob, 3.3, 9.2, earTilt, ...k);
    c.fillRotatedEllipse(35.2, 10.0 + bob, 3.2, 9.0, earTilt + 13, ...k);
    // Bright round eye cutout
    c.cutoutCircle(37.0, 21.8 + bob, 2.1);
    return c.buf;
  }

  if (pose.startsWith('run_') || pose.startsWith('sprint_')) {
    const isSprint = pose.startsWith('sprint_');
    const step = Number(pose.split('_')[1]); // 1..4
    const yArc = [0, -3.0, -4.8, -1.6, 0.8][step];
    const bodyTilt = isSprint ? -14 : [0, -12, -5, 8, 12][step];
    const frontAngle = [0, 38, 60, -22, -48][step];
    const backAngle = [0, -48, -64, 22, 48][step];

    // Torso
    c.fillRotatedEllipse(28, 28 + yArc, isSprint ? 14.0 : 12.6, 7.4, bodyTilt, ...k);
    // Tail
    c.fillCircle(13.5, 27 + yArc + bodyTilt * 0.14, 4.0, ...k);
    // Head
    c.fillRotatedEllipse(40.5, 21.5 + yArc * 0.9, 7.8, 6.6, 8, ...k);
    // Swept ears
    const earSweep = isSprint ? -56 : -38 + (step % 2) * 7;
    c.fillRotatedEllipse(31.5, 13.0 + yArc * 0.9, 3.0, 8.8, earSweep, ...k);
    c.fillRotatedEllipse(34.5, 11.8 + yArc * 0.9, 2.9, 8.6, earSweep + 11, ...k);
    // Eye cutout
    c.cutoutCircle(43.0, 20.4 + yArc * 0.9, 2.0);

    // Front & back legs
    const fx = 36 + Math.sin((frontAngle * Math.PI) / 180) * 8.5;
    const fy = 37 + yArc + Math.cos((frontAngle * Math.PI) / 180) * 4.2;
    c.fillCapsule(35, 32 + yArc, fx, fy, 2.7, ...k);

    const bx = 19 + Math.sin((backAngle * Math.PI) / 180) * 9.5;
    const by = 37 + yArc + Math.cos((backAngle * Math.PI) / 180) * 4.2;
    c.fillCapsule(21, 32 + yArc, bx, by, 2.9, ...k);

    // Speed lines
    if (isSprint) {
      const offset = step % 2 === 0 ? 0 : 2.5;
      c.fillCapsule(2 + offset, 19, 10 + offset, 19, 1.4, ...k);
      c.fillCapsule(1 + offset, 27, 8 + offset, 27, 1.4, ...k);
    }
    return c.buf;
  }

  if (pose === 'alert_1' || pose === 'alert_2') {
    const jump = pose === 'alert_2' ? -2.4 : 0;
    // Tall alert rabbit
    c.fillRotatedEllipse(21, 33 + jump, 10.2, 10.2, -5, ...k);
    c.fillCircle(10, 37 + jump, 4.0, ...k);
    c.fillRotatedEllipse(24, 21.5 + jump, 8.4, 7.2, 0, ...k);
    // Straight V-ears
    c.fillRotatedEllipse(19.5, 9.0 + jump, 3.1, 9.2, -10, ...k);
    c.fillRotatedEllipse(28.5, 9.0 + jump, 3.1, 9.2, 10, ...k);
    // Alert wide eye cutout
    c.cutoutCircle(27.2, 20.6 + jump, 2.2);
    // Bold exclamation mark (!)
    c.fillCapsule(44, 8 + jump, 44, 24 + jump, 2.8, ...k);
    c.fillCircle(44, 33 + jump, 3.1, ...k);
    return c.buf;
  }

  // exhausted_1
  c.fillRotatedEllipse(24, 34.5, 14.0, 8.2, 0, ...k);
  c.fillCircle(9.5, 36, 3.8, ...k);
  c.fillRotatedEllipse(35, 31, 8.0, 7.0, 16, ...k);
  // Droopy ears
  c.fillRotatedEllipse(29, 31, 3.0, 7.8, 35, ...k);
  c.fillRotatedEllipse(32.5, 32.5, 2.8, 7.4, 24, ...k);
  // Tired half-closed eye cutout
  c.cutoutCapsule(34.5, 29.5, 39.5, 31.3, 1.2);
  // Sweat drop
  c.fillCircle(46, 20, 2.8, ...k);
  c.fillCapsule(46, 14, 46, 19.5, 1.5, ...k);
  return c.buf;
}

// Draw 512x512 cute pastel app icon for Tauri bundle
function drawAppIcon() {
  const s = 512;
  const c = createCanvas(s, s);
  // Soft rounded-ish pastel lavender-blue background circle
  c.fillCircle(256, 256, 236, 129, 140, 248, 1);
  c.fillCircle(256, 256, 216, 248, 250, 252, 1);
  // Pastel bunny head + ears
  c.fillRotatedEllipse(196, 155, 34, 96, -10, 255, 255, 255, 1);
  c.fillRotatedEllipse(316, 155, 34, 96, 10, 255, 255, 255, 1);
  // Inner pink ears
  c.fillRotatedEllipse(196, 162, 16, 60, -10, 251, 182, 206, 1);
  c.fillRotatedEllipse(316, 162, 16, 60, 10, 251, 182, 206, 1);
  // Head
  c.fillRotatedEllipse(256, 295, 125, 105, 0, 255, 255, 255, 1);
  // Eyes
  c.fillCircle(210, 290, 14, 51, 65, 85, 1);
  c.fillCircle(302, 290, 14, 51, 65, 85, 1);
  // Cheeks
  c.fillRotatedEllipse(180, 322, 22, 13, 0, 253, 164, 175, 0.75);
  c.fillRotatedEllipse(332, 322, 22, 13, 0, 253, 164, 175, 0.75);
  // Nose
  c.fillRotatedEllipse(256, 312, 11, 8, 0, 244, 114, 182, 1);
  return c.buf;
}

const poses = [
  'sleep_1',
  'sleep_2',
  'idle_1',
  'idle_2',
  'run_1',
  'run_2',
  'run_3',
  'run_4',
  'sprint_1',
  'sprint_2',
  'sprint_3',
  'sprint_4',
  'alert_1',
  'alert_2',
  'exhausted_1',
];

for (const pose of poses) {
  const rgba = drawTrayRabbit(pose);
  const png = encodePng(56, 44, rgba);
  fs.writeFileSync(path.join(SPRITES_DIR, `${pose}.png`), png);
}

fs.writeFileSync(path.join(ICONS_DIR, 'icon.png'), encodePng(512, 512, drawAppIcon()));
console.log(`Generated ${poses.length} tray sprites in assets/sprites/ and src-tauri/icons/icon.png`);
