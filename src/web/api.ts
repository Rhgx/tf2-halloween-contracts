// Talking to the local server. The response types come straight from the server code, so the two
// can't drift apart; the imports are type-only and add nothing to the bundle.

import type { GameState } from '../server/game.ts';
import type { ServerState } from '../server/main.ts';

export type { GameState, ServerState };

export async function api<T = ServerState>(path: string, init?: RequestInit): Promise<T> {
  let res: Response;
  try {
    res = await fetch(path, init);
  } catch {
    throw new Error("The app's server isn't running. Start the app again.");
  }
  const body: unknown = await res.json();
  if (!res.ok) {
    const error = typeof body === 'object' && body !== null && 'error' in body ? String(body.error) : res.statusText;
    throw new Error(error);
  }
  return body as T;
}

export function errorMessage(err: unknown) {
  return err instanceof Error ? err.message : String(err);
}
