// Finds the TF2 install: every library in Steam's libraryfolders.vdf, then Steam's own folder. On
// Windows Steam is found through the registry, then the default install path; on Linux through
// the usual install paths.

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { parseKV } from './kv.ts';

const HOME = os.homedir();
// Native installs (~/.steam/steam usually links to the first), then Flatpak and Snap.
const LINUX_STEAM = [
  path.join(HOME, '.local/share/Steam'),
  path.join(HOME, '.steam/steam'),
  path.join(HOME, '.var/app/com.valvesoftware.Steam/.local/share/Steam'),
  path.join(HOME, 'snap/steam/common/.local/share/Steam')
];
const DEFAULT_STEAM = 'C:/Program Files (x86)/Steam';

function steamDirs(): string[] {
  if (process.platform !== 'win32') return LINUX_STEAM.filter((dir) => fs.existsSync(dir));
  try {
    const out = execFileSync('reg', ['query', 'HKCU\\Software\\Valve\\Steam', '/v', 'SteamPath'], {
      encoding: 'utf8',
      windowsHide: true
    });
    const dir = out.match(/SteamPath\s+REG_SZ\s+(.+)/)?.[1].trim();
    return [...(dir ? [dir] : []), DEFAULT_STEAM];
  } catch {
    return [DEFAULT_STEAM];
  }
}

function libraryDirs(steam: string): string[] {
  try {
    const folders = parseKV(
      fs.readFileSync(path.join(steam, 'steamapps', 'libraryfolders.vdf'), 'utf8')
    ).libraryfolders;
    if (typeof folders !== 'object' || Array.isArray(folders)) return [];
    return Object.values(folders).flatMap((library) =>
      typeof library === 'object' && !Array.isArray(library) && typeof library.path === 'string' ? [library.path] : []
    );
  } catch {
    return [];
  }
}

export function findTf2Dir(): string | null {
  const candidates = steamDirs().flatMap((steam) => [...libraryDirs(steam), steam]);
  for (const library of candidates) {
    const dir = path.join(library, 'steamapps', 'common', 'Team Fortress 2');
    if (fs.existsSync(path.join(dir, 'tf'))) return dir;
  }
  return null;
}
