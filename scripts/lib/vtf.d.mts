export interface DecodedVTF {
  width: number;
  height: number;
  rgba: Uint8Array;
  format: number;
}

export function decodeVTF(input: Uint8Array | ArrayBuffer): DecodedVTF;
