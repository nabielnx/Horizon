// Optional authoring step. Committed ICO/PNGs are sufficient for Cargo builds.
const fs = require('node:fs');
const path = require('node:path');
const { Resvg } = require('@resvg/resvg-js');
const root = path.resolve(__dirname, '../assets/branding');
const eye = fs.readFileSync(path.join(root, 'eye.svg'), 'utf8').match(/<path[^>]*\/>/)[0];
const sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256];
const images = sizes.map(size => {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 200 200">
    <defs>
      <linearGradient id="tile" x2=".8" y2="1"><stop stop-color="#292a2d"/><stop offset="1" stop-color="#101113"/></linearGradient>
      <linearGradient id="glass" x1="6" y1="0" x2="8" y2="8" gradientUnits="userSpaceOnUse">
        <stop stop-color="#ffffff"/><stop offset=".32" stop-color="#d3dce7"/>
        <stop offset=".82" stop-color="#768293"/><stop offset="1" stop-color="#cad4e0"/>
      </linearGradient>
      <g id="eye" fill="url(#glass)">${eye}</g>
    </defs>
    <rect x="4" y="4" width="192" height="192" rx="48" fill="url(#tile)"/>
    <rect x="5.5" y="5.5" width="189" height="189" rx="46.5" fill="none" stroke="#747479" stroke-opacity=".65" stroke-width="3"/>
    <use href="#eye" transform="translate(29 84) scale(3.8)"/>
    <use href="#eye" transform="translate(171 84) scale(-3.8 3.8)"/>
    </svg>`;
  const png = new Resvg(svg, { font: { loadSystemFonts: false } }).render().asPng();
  if (size === 256) fs.writeFileSync(path.join(root, 'app-icon.png'), png);
  return png;
});
const header = Buffer.alloc(6 + 16 * sizes.length);
header.writeUInt16LE(1, 2);
header.writeUInt16LE(sizes.length, 4);
let offset = header.length;
images.forEach((png, i) => {
  const entry = 6 + 16 * i;
  header[entry] = header[entry + 1] = sizes[i] === 256 ? 0 : sizes[i];
  header.writeUInt16LE(1, entry + 4);
  header.writeUInt16LE(32, entry + 6);
  header.writeUInt32LE(png.length, entry + 8);
  header.writeUInt32LE(offset, entry + 12);
  offset += png.length;
});
fs.writeFileSync(path.join(root, 'horizon.ico'), Buffer.concat([header, ...images]));
console.log(`Prepared Horizon icon (${sizes.join(', ')}px).`);
