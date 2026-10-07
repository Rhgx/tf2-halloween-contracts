// Minimal Valve KeyValues (text) parser, ported from warpaint-viewer/tools/lib/kv.mjs.
// Handles quoted/unquoted tokens, nested { } blocks and // comments. Duplicate keys in a block
// collapse into an array of values (order preserved).

export type KVValue = string | KV | KVValue[];
export type KV = { [key: string]: KVValue };

export function parseKV(text: string): KV {
  let i = 0;
  const n = text.length;

  function skipWs() {
    while (i < n) {
      const c = text[i];
      if (c === ' ' || c === '\t' || c === '\r' || c === '\n') {
        i++;
      } else if (c === '/' && text[i + 1] === '/') {
        while (i < n && text[i] !== '\n') i++;
      } else {
        break;
      }
    }
  }

  function readToken(): '{' | '}' | { str: string } | null {
    skipWs();
    if (i >= n) return null;
    const c = text[i];
    if (c === '{' || c === '}') {
      i++;
      return c;
    }
    let s = '';
    if (c === '"') {
      i++;
      while (i < n && text[i] !== '"') {
        if (text[i] === '\\' && (text[i + 1] === '"' || text[i + 1] === '\\')) {
          s += text[i + 1];
          i += 2;
          continue;
        }
        s += text[i++];
      }
      i++;
      return { str: s };
    }
    while (i < n && !/[\s{}"]/.test(text[i]) && !(text[i] === '/' && text[i + 1] === '/')) {
      s += text[i++];
    }
    return { str: s };
  }

  function addKey(obj: KV, key: string, value: KVValue) {
    if (!Object.hasOwn(obj, key)) {
      obj[key] = value;
      return;
    }
    const existing = obj[key];
    if (Array.isArray(existing)) existing.push(value);
    else obj[key] = [existing, value];
  }

  function parseBlock(): KV {
    const obj: KV = {};
    while (true) {
      const key = readToken();
      if (key === null || key === '}') break;
      if (key === '{') continue;
      const val = readToken();
      if (val === null) break;
      if (val === '{') {
        addKey(obj, key.str, parseBlock());
      } else if (val === '}') {
        addKey(obj, key.str, '');
        break;
      } else if (key.str[0] !== '#') {
        addKey(obj, key.str, val.str);
      }
    }
    return obj;
  }

  return parseBlock();
}
