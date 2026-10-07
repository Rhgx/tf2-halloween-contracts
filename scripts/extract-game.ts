// Builds src/halloween-maps.json and public/maps/* from a local TF2 install plus the TF2 Wiki:
// map list and casual menu placement from items_game.txt, names and modes from tf_english.txt,
// ConTracker contract nodes from proto_defs.vpd, screenshots from each map's wiki infobox. Maps
// the wiki has no screenshot for yet fall back to the game's own casual menu thumbnail from
// tf2_textures_dir.vpk (via Valve's vpk.exe).

import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import sharp, { type Sharp } from 'sharp';

import { parseKV, type KV, type KVValue } from '../src/lib/kv.ts';
import { decode, messages, num, strings, type Fields } from '../src/lib/protobuf.ts';
import { findTf2Dir } from '../src/lib/tf2-dir.ts';
import type { HalloweenMap } from '../src/maps.ts';
import { decodeVTF } from './lib/vtf.mjs';

const TF2_DIR = process.env.TF2_DIR ?? findTf2Dir() ?? '';
if (!TF2_DIR) throw new Error('TF2 was not found in your Steam libraries. Set TF2_DIR to its folder.');
const VPK_EXE = path.join(TF2_DIR, 'bin', 'vpk.exe');
const TEXTURES_VPK = path.join(TF2_DIR, 'tf', 'tf2_textures_dir.vpk');
const PROTO_DEFS = path.join(TF2_DIR, 'tf', 'scripts', 'protodefs', 'proto_defs.vpd');

const ROOT = path.resolve(import.meta.dirname, '..');
const OUT_JSON = path.join(ROOT, 'src', 'halloween-maps.json');
const OUT_IMAGES = path.join(ROOT, 'public', 'maps');

const WIKI = 'https://wiki.teamfortress.com';
const WIKI_IMAGE_WIDTH = 640;
const USER_AGENT = 'tf2-halloween-contracts map list extract (https://github.com/Rhgx/tf2-halloween-contracts)';

// Casual menu categories that hold the Halloween maps, in menu order.
const SECTIONS = ['featured', 'halloween'];

// Map prefix -> localization token the game uses for that mode.
const MODE_TOKENS: Record<string, string> = {
  arena: 'Gametype_Arena',
  cp: 'Gametype_CP',
  ctf: 'Gametype_CTF',
  htf: 'GameType_HTF',
  koth: 'Gametype_Koth',
  pd: 'Gametype_PlayerDestruction',
  pl: 'Gametype_Escort',
  plr: 'Gametype_EscortRace',
  sd: 'Gametype_SD',
  tow: 'GameType_TOW',
  vsh: 'GameType_VSH',
  zi: 'GameType_ZI'
};

// Maps whose display name is taken on the wiki by an item page.
const WIKI_TITLES: Record<string, string> = {
  pl_fifthcurve_event: 'Brimstone_(map)',
  plr_hacksaw_event: 'Bonesaw_(map)'
};

type WikiPage = {
  title: string;
  revisions?: { '*': string }[];
  imageinfo?: { url?: string; thumburl?: string }[];
};

function block(value: KVValue | undefined, label: string): KV {
  if (value && typeof value === 'object' && !Array.isArray(value)) return value;
  throw new Error(`items_game.txt: expected block at ${label}`);
}

function text(value: KVValue | undefined): string | undefined {
  return typeof value === 'string' ? value : undefined;
}

// Values of a block whose keys may repeat ("1", "2", ...), flattened in order.
function entries(kv: KV): KV[] {
  return Object.values(kv)
    .flat()
    .filter((v): v is KV => typeof v === 'object' && !Array.isArray(v));
}

function loadLocalization(file: string) {
  // tf_english.txt is UTF-16 LE with a BOM.
  const content = fs.readFileSync(file).subarray(2).toString('utf16le');
  const tokens = new Map<string, string>();
  for (const m of content.matchAll(/"((?:[^"\\]|\\.)*)"\s*"((?:[^"\\]|\\.)*)"/g)) {
    tokens.set(m[1].toLowerCase(), m[2]);
  }
  return (token: string | undefined) => {
    const value = token ? tokens.get(token.replace(/^#/, '').toLowerCase()) : undefined;
    if (value === undefined) throw new Error(`tf_english.txt: missing token ${token}`);
    return value;
  };
}

async function fetchOk(url: string | URL) {
  const res = await fetch(url, { headers: { 'User-Agent': USER_AGENT } });
  if (!res.ok) throw new Error(`${res.status} ${res.statusText} for ${url}`);
  return res;
}

// Runs a MediaWiki query in batches and returns pages keyed by the title we asked for.
async function wikiPages(params: Record<string, string>, titles: string[]) {
  const pages = new Map<string, WikiPage>();
  for (let i = 0; i < titles.length; i += 50) {
    const batch = titles.slice(i, i + 50);
    const url = new URL('/w/api.php', WIKI);
    url.search = new URLSearchParams({
      action: 'query',
      format: 'json',
      ...params,
      titles: batch.join('|')
    }).toString();

    const data: unknown = await (await fetchOk(url)).json();
    const query = (
      data as { query?: { normalized?: { from: string; to: string }[]; pages?: Record<string, WikiPage> } }
    ).query;
    if (!query?.pages) throw new Error(`Unexpected wiki response for ${url}`);

    const normalized = new Map(query.normalized?.map((n) => [n.from, n.to]));
    const byTitle = new Map(Object.values(query.pages).map((page) => [page.title, page]));
    for (const title of batch) {
      const page = byTitle.get(normalized.get(title) ?? title);
      if (page) pages.set(title, page);
    }
  }
  return pages;
}

// Screenshot URL per map code, resized by the wiki to WIKI_IMAGE_WIDTH. Uses the map page's
// infobox image, else the wiki's "<Code>.png" naming convention (new maps often have the
// screenshot uploaded before the infobox is filled in).
async function wikiScreenshots(maps: { code: string; wikiTitle: string }[]) {
  const sources = await wikiPages(
    { prop: 'revisions', rvprop: 'content', rvsection: '0' },
    maps.map((m) => m.wikiTitle)
  );
  const candidates = new Map<string, string[]>();
  for (const { code, wikiTitle } of maps) {
    const source = sources.get(wikiTitle)?.revisions?.[0]?.['*'] ?? '';
    const infobox = source.match(/\|\s*map-image\s*=\s*([^\n|]*)/)?.[1]?.trim();
    candidates.set(code, [...(infobox ? [`File:${infobox}`] : []), `File:${code}.png`]);
  }

  const infos = await wikiPages({ prop: 'imageinfo', iiprop: 'url', iiurlwidth: String(WIKI_IMAGE_WIDTH) }, [
    ...new Set([...candidates.values()].flat())
  ]);
  const urls = new Map<string, string>();
  for (const [code, files] of candidates) {
    for (const file of files) {
      const info = infos.get(file)?.imageinfo?.[0];
      // The wiki occasionally fails to render a thumbnail; the original still works.
      const url = info?.thumburl ?? info?.url;
      if (url) {
        urls.set(code, url);
        break;
      }
    }
  }
  return urls;
}

// proto_defs.vpd: repeated [type:int32][count:int32], then per definition [size:int32][protobuf].
// See ProtoDefTypes and the CMsg*Def messages in the SDK's tf_proto_def_messages.proto.
function loadProtoDefs() {
  const buf = fs.readFileSync(PROTO_DEFS);
  const defs = new Map<number, Map<number, Fields>>();
  let i = 0;
  while (i < buf.length) {
    const type = buf.readInt32LE(i);
    const count = buf.readInt32LE(i + 4);
    i += 8;
    const byIndex = new Map<number, Fields>();
    defs.set(type, byIndex);
    for (let n = 0; n < count; n++) {
      const size = buf.readInt32LE(i);
      const def = decode(buf.subarray(i + 4, i + 4 + size));
      i += 4 + size;
      // Every definition starts with CMsgProtoDefHeader { defindex = 1 }.
      byIndex.set(num(messages(def, 1)[0], 1) ?? -1, def);
    }
  }
  return (type: number) => defs.get(type) ?? new Map<number, Fields>();
}

// Map restrictions inside an items_game quest_objective_conditions block, e.g. { type map value x }.
function conditionMaps(value: KVValue | undefined): string[] {
  if (!value || typeof value === 'string') return [];
  if (Array.isArray(value)) return value.flatMap(conditionMaps);
  const own = text(value.type) === 'map' && text(value.value) ? [text(value.value)!] : [];
  return [...own, ...Object.values(value).flatMap(conditionMaps)];
}

// ConTracker node defindex and folder per map code. A node's maps come from its offered quests' objectives:
// the objective's own map list plus map conditions in items_game.txt. Its matchmaking hint is
// ignored because some are stale (Afterlife points at Perks). Ties and misses fall back to the
// node's editor name matching the map name (Mann Manor vs. the HHH boss node, Marshlands whose
// objective still names sd_marshlands). The folder is the node's region as the ConTracker labels
// it, e.g. "community_maps_2".
function contractNodes(game: KV, maps: { code: string; name: string }[]) {
  const def = loadProtoDefs();
  // Proto def strings are localized by "<type>_<defindex>_<field id>" tokens; region name is field 2.
  const protoLoc = loadLocalization(path.join(TF2_DIR, 'tf/resource/tf_proto_obj_defs_english.txt'));
  const conditions = block(game.quest_objective_conditions, 'quest_objective_conditions');
  const ids = (fields: Fields, field: number) => messages(fields, field).map((id) => num(id, 1));

  // CMsgQuestMapNodeDef.quest_options = 11 -> CMsgQuestDef { map = 17, objectives = 16 }
  // -> ObjectiveInstance.objective = 1 -> CMsgQuestObjectiveDef { map = 8, conditions_defindex = 5 }.
  const nodes = [...def(0)].map(([defindex, node]) => {
    const quests = ids(node, 11).flatMap((id) => def(4).get(id ?? -1) ?? []);
    const objectives = quests
      .flatMap((quest) => messages(quest, 16).flatMap((instance) => ids(instance, 1)))
      .flatMap((id) => def(5).get(id ?? -1) ?? []);
    const nodeMaps = new Set([
      ...quests.flatMap((quest) => strings(quest, 17)),
      ...objectives.flatMap((objective) => [
        ...strings(objective, 8),
        ...conditionMaps(conditions[String(num(objective, 5))])
      ])
    ]);
    const region = ids(node, 10)[0];
    return { defindex, region, name: strings(messages(node, 1)[0], 2)[0] ?? '', maps: nodeMaps };
  });

  const result = new Map<string, { contractNode: number; contractFolder: string }>();
  for (const { code, name } of maps) {
    const byMap = nodes.filter((node) => node.maps.has(code));
    const byName = (byMap.length ? byMap : nodes).filter((node) => node.name === name);
    const match = byMap.length === 1 ? byMap : byName;
    if (match.length !== 1) {
      const found = (match.length ? match : byMap).map((node) => `${node.defindex} ${node.name}`).join(', ');
      throw new Error(`proto_defs.vpd: no single contract node for ${code} (${found || 'none'})`);
    }
    result.set(code, {
      contractNode: match[0].defindex,
      contractFolder: protoLoc(`3_${match[0].region}_field { field_number: 2 }`)
    });
  }
  return result;
}

// Lossless WebP keeps every pixel and is about a third smaller than PNG. JPEGs from the wiki are
// saved as they are, since a lossless copy only preserves their artifacts at about five times the size.
const toWebp = (image: Sharp) => image.webp({ lossless: true, effort: 6 }).toBuffer();

// The casual menu thumbnails for the given maps, decoded. They're square textures; only the top
// 4:3 area holds the picture.
function menuThumbs(codes: string[]) {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'tf2-halloween-'));
  try {
    const rels = codes.map((code) => `materials/vgui/maps/menu_thumb_${code}.vtf`);
    fs.mkdirSync(path.join(tmp, 'materials/vgui/maps'), { recursive: true });
    // vpk.exe writes relative to cwd; batch to stay under the command-line length limit.
    for (let i = 0; i < rels.length; i += 40) {
      execFileSync(VPK_EXE, ['x', TEXTURES_VPK, ...rels.slice(i, i + 40)], { cwd: tmp, stdio: 'ignore' });
    }
    return new Map(
      codes.map((code, i) => {
        const file = path.join(tmp, rels[i]);
        if (!fs.existsSync(file)) throw new Error(`No wiki screenshot and no menu thumbnail for ${code}`);
        const { rgba, width } = decodeVTF(fs.readFileSync(file));
        const height = (width * 3) / 4;
        return [code, sharp(rgba.subarray(0, width * height * 4), { raw: { width, height, channels: 4 } })];
      })
    );
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
}

async function main() {
  const game = block(
    parseKV(fs.readFileSync(path.join(TF2_DIR, 'tf/scripts/items/items_game.txt'), 'utf8')).items_game,
    'items_game'
  );
  const loc = loadLocalization(path.join(TF2_DIR, 'tf/resource/tf_english.txt'));

  // Keyed by map name; the block's own key is the map's bit in casual_criteria.vdf.
  const master = new Map(
    Object.entries(block(game.master_maps_list, 'master_maps_list')).map(([index, value]) => {
      const info = block(value, `master_maps_list.${index}`);
      return [text(info.name), { info, index: Number(index) }];
    })
  );
  const categories = block(game.maps, 'maps');
  const tabs = block(game.matchmaking_categories, 'matchmaking_categories');

  const maps: (Omit<HalloweenMap, 'image' | 'contractNode' | 'contractFolder'> & { wikiTitle: string })[] = [];
  const seen = new Set<string>();

  for (const sectionKey of SECTIONS) {
    const category = block(categories[sectionKey], `maps.${sectionKey}`);
    const tabKey = text(category.mm_type) ?? '';
    const tab = loc(text(block(tabs[tabKey], `matchmaking_categories.${tabKey}`).localized_name));
    const section = loc(text(category.localized_name));

    for (const entry of entries(block(category.maplist, `maps.${sectionKey}.maplist`))) {
      const code = text(entry.name);
      if (!code || text(entry.enabled) !== '1' || seen.has(code)) continue;
      seen.add(code);

      const listed = master.get(code);
      if (!listed) throw new Error(`master_maps_list: missing ${code}`);

      const prefix = code.split('_')[0];
      const modeToken = MODE_TOKENS[prefix];
      if (!modeToken) throw new Error(`No game mode known for prefix "${prefix}" (${code})`);

      const name = loc(text(listed.info.localizedname));
      // The wiki titles community-mode maps without the "(ZI)"-style suffix the menu adds.
      const wikiTitle = WIKI_TITLES[code] ?? name.replace(/ \([A-Z]+\)$/, '').replaceAll(' ', '_');

      maps.push({
        code,
        name,
        tab,
        section,
        mode: loc(modeToken),
        mapIndex: listed.index,
        wikiUrl: `${WIKI}/wiki/${encodeURIComponent(wikiTitle)}`,
        wikiTitle
      });
    }
  }

  const nodes = contractNodes(game, maps);
  const screenshots = await wikiScreenshots(maps);

  fs.rmSync(OUT_IMAGES, { recursive: true, force: true });
  fs.mkdirSync(OUT_IMAGES, { recursive: true });

  const images = new Map<string, string>();
  for (const map of maps) {
    const url = screenshots.get(map.code);
    if (!url) continue;
    const data = Buffer.from(await (await fetchOk(url)).arrayBuffer());
    const ext = path.extname(new URL(url).pathname).toLowerCase();
    const jpeg = ext === '.jpg' || ext === '.jpeg';
    const file = `${map.code}${jpeg ? ext : '.webp'}`;
    fs.writeFileSync(path.join(OUT_IMAGES, file), jpeg ? data : await toWebp(sharp(data)));
    images.set(map.code, `/maps/${file}`);
  }

  const fallback = maps.filter((m) => !images.has(m.code)).map((m) => m.code);
  for (const [code, thumb] of menuThumbs(fallback)) {
    fs.writeFileSync(path.join(OUT_IMAGES, `${code}.webp`), await toWebp(thumb));
    images.set(code, `/maps/${code}.webp`);
  }

  const output: HalloweenMap[] = maps.map(({ wikiTitle: _wikiTitle, ...map }) => ({
    ...map,
    ...nodes.get(map.code)!,
    image: images.get(map.code) ?? ''
  }));
  fs.writeFileSync(OUT_JSON, `${JSON.stringify(output, null, 2)}\n`);

  console.log(`Wrote ${output.length} maps to ${OUT_JSON}`);
  console.log(
    `Images: ${output.length - fallback.length} wiki screenshots, ${fallback.length} game thumbnails (${fallback.join(', ')})`
  );
}

await main();
