// Finds the TF2 install: Steam's own folder from the registry, then every library in its
// libraryfolders.vdf, then the default install path.

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

import { parseKV } from './kv.ts';

const DEFAULT_STEAM = 'C:/Program Files (x86)/Steam';

function steamDir(): string | null {
  try {
    const out = execFileSync('reg', ['query', 'HKCU\\Software\\Valve\\Steam', '/v', 'SteamPath'], {
      encoding: 'utf8',
      windowsHide: true
    });
    return out.match(/SteamPath\s+REG_SZ\s+(.+)/)?.[1].trim() ?? null;
  } catch {
    return null;
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
  const steam = steamDir();
  const candidates = [...(steam ? [...libraryDirs(steam), steam] : []), DEFAULT_STEAM];
  for (const library of candidates) {
    const dir = path.join(library, 'steamapps', 'common', 'Team Fortress 2');
    if (fs.existsSync(path.join(dir, 'tf'))) return dir;
  }
  return null;
}
