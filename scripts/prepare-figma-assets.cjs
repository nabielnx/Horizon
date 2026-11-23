// Convert the original-size bitmaps used by the renderer and its tests.
const fs = require('node:fs');
const path = require('node:path');
const { Resvg } = require('@resvg/resvg-js');
const root = path.resolve(__dirname, '../assets/figma');
const bitmapNames = [
  'delete-event',
  'edit-event',
  'fire',
  'media-lyrics-glyph',
  'media-next',
  'media-pause-glyph',
  'media-previous',
  'nav-music',
  'peek-pause-glyph',
  'repeat',
  'shell', // Native bitmap size fixture.
  'spotify',
];
for (const name of bitmapNames) {
  const render = new Resvg(fs.readFileSync(path.join(root, `${name}.svg`)), { font: { loadSystemFonts: false } });
  fs.writeFileSync(path.join(root, `${name}.png`), render.render().asPng());
}
