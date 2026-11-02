// Build-time conversion only. Horizon bundles the resulting original-size PNGs.
const fs = require('node:fs');
const path = require('node:path');
const { Resvg } = require('@resvg/resvg-js');
const root = path.resolve(__dirname, '../assets/figma');
for (const file of fs.readdirSync(root).filter(name => name.endsWith('.svg'))) {
  const render = new Resvg(fs.readFileSync(path.join(root, file)), { font: { loadSystemFonts: false } });
  fs.writeFileSync(path.join(root, file.replace(/\.svg$/, '.png')), render.render().asPng());
}
