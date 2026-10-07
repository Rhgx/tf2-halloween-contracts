// Builds build/tf2-halloween-contracts.exe with Node's single executable application support: the
// server (src/server) bundled into one file by esbuild and the built UI embedded as assets, written
// into a copy of the Node binary running this script by node --build-sea. Run "npm run build" first
// (npm run package does).

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { build } from 'esbuild';

const ROOT = path.resolve(import.meta.dirname, '..');
const DIST = path.join(ROOT, 'dist');
const OUT = path.join(ROOT, 'build');
const EXE = path.join(OUT, 'tf2-halloween-contracts.exe');

// Library lines a single-file bundle can't run as written. Each must still match, or the library
// changed and the patch needs a look.
type Patch = { file: RegExp; from: string; to: (dir: string) => string };
const PATCHES: Patch[] = [
  {
    // Reads Steam's public key from a file next to its source; inline it instead.
    file: /[\\/]steam-crypto[\\/]index\.js$/,
    from: "require('fs').readFileSync(__dirname + '/system.pem')",
    to: (dir) => `Buffer.from(${JSON.stringify(fs.readFileSync(path.join(dir, 'system.pem'), 'utf8'))})`
  },
  {
    // A require by variable can't be bundled, and the exe's require only knows built-ins. The pure
    // JS lzma is what's installed, so name it outright.
    file: /[\\/]steam-user[\\/]components[\\/]cdn_compression\.js$/,
    from: "requireWithFallback('lzma-native', 'lzma')",
    to: () => "require('lzma')"
  },
  {
    // Same again inside lzma: its worker is required by a computed path.
    file: /[\\/]lzma[\\/]index\.js$/,
    from: 'require(require("path").join(__dirname, "src" ,"lzma_worker.js"))',
    to: () => 'require("./src/lzma_worker.js")'
  }
];

if (!fs.existsSync(path.join(DIST, 'index.html'))) throw new Error('dist/ is missing. Run "npm run build" first.');
fs.rmSync(OUT, { recursive: true, force: true });
fs.mkdirSync(OUT);

await build({
  entryPoints: [path.join(ROOT, 'src', 'server', 'main.ts')],
  outfile: path.join(OUT, 'server.cjs'),
  bundle: true,
  platform: 'node',
  format: 'cjs',
  target: 'node26',
  define: { 'import.meta.dirname': '__dirname' },
  // A library's deprecated Buffer() call would otherwise greet users with a warning at startup.
  banner: { js: 'process.noDeprecation = true;' },
  logLevel: 'warning',
  plugins: [
    {
      name: 'patch-for-single-file',
      setup(build) {
        for (const patch of PATCHES) {
          build.onLoad({ filter: patch.file }, (args) => {
            const source = fs.readFileSync(args.path, 'utf8');
            if (!source.includes(patch.from)) throw new Error(`${args.path} changed; update PATCHES in package.ts.`);
            return { contents: source.replace(patch.from, patch.to(path.dirname(args.path))), loader: 'js' };
          });
        }
      }
    }
  ]
});

// Every built file, keyed by its path inside dist, the way the server asks for them.
const assets: Record<string, string> = {};
for (const file of fs.readdirSync(DIST, { recursive: true, withFileTypes: true })) {
  if (!file.isFile()) continue;
  const full = path.join(file.parentPath, file.name);
  assets[path.relative(DIST, full).split(path.sep).join('/')] = full;
}

const config = path.join(OUT, 'sea-config.json');
fs.writeFileSync(
  config,
  JSON.stringify({
    main: path.join(OUT, 'server.cjs'),
    output: EXE,
    disableExperimentalSEAWarning: true,
    assets
  })
);
execFileSync(process.execPath, ['--build-sea', config], { stdio: 'inherit' });

const mb = (fs.statSync(EXE).size / 1024 / 1024).toFixed(0);
console.log(`Built ${path.relative(ROOT, EXE)} (${mb} MB, ${Object.keys(assets).length} embedded files).`);
