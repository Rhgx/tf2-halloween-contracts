// Builds build/tf2-halloween-contracts(.exe) with Node's single executable application support: the
// server (src/server) bundled into one file by esbuild and the built UI embedded as assets, written
// into a copy of the Node binary running this script by node --build-sea. On Windows the exe is then
// given the app's icon and name. The build is for the platform it runs on: package on Linux for the
// Linux binary. Run "npm run build" first (npm run package does).

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { build } from 'esbuild';
import { Data, NtExecutable, NtExecutableResource, Resource } from 'resedit';

const ROOT = path.resolve(import.meta.dirname, '..');
const DIST = path.join(ROOT, 'dist');
const OUT = path.join(ROOT, 'build');
const EXE = path.join(OUT, process.platform === 'win32' ? 'tf2-halloween-contracts.exe' : 'tf2-halloween-contracts');

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

if (process.platform === 'win32') {
  // The exe starts out as Node: its icon, and its name in Task Manager and the file's properties.
  // Swap in ours. The rest of the resources, the embedded app among them, are written back as they
  // were. Node's company and MIT copyright lines stay, since the binary is still Node.
  const exe = NtExecutable.from(fs.readFileSync(EXE), { ignoreCert: true });
  const resources = NtExecutableResource.from(exe);
  const icons = Data.IconFile.from(fs.readFileSync(path.join(ROOT, 'assets', 'icon.ico'))).icons.map(
    (icon) => icon.data
  );
  const groups = Resource.IconGroupEntry.fromEntries(resources.entries);
  if (groups.length === 0) throw new Error('No icon found in the Node binary to replace.');
  for (const group of groups)
    Resource.IconGroupEntry.replaceIconsForResource(resources.entries, group.id, group.lang, icons);

  const { version } = JSON.parse(fs.readFileSync(path.join(ROOT, 'package.json'), 'utf8')) as { version: string };
  const [info] = Resource.VersionInfo.fromEntries(resources.entries);
  if (!info) throw new Error('No version info found in the Node binary to replace.');
  for (const language of info.getAllLanguagesForStringValues()) {
    info.setStringValues(language, {
      ProductName: 'TF2 Halloween Contracts',
      FileDescription: 'TF2 Halloween Contracts',
      InternalName: 'tf2-halloween-contracts',
      OriginalFilename: path.basename(EXE)
    });
    info.setFileVersion(version, language.lang);
    info.setProductVersion(version, language.lang);
  }
  info.outputToResourceEntries(resources.entries);
  resources.outputResource(exe);
  fs.writeFileSync(EXE, Buffer.from(exe.generate()));
}

const mb = (fs.statSync(EXE).size / 1024 / 1024).toFixed(0);
console.log(`Built ${path.relative(ROOT, EXE)} (${mb} MB, ${Object.keys(assets).length} embedded files).`);
