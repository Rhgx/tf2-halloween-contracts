// Minimal protobuf wire-format reader. Decodes one message level into field number -> values
// (varints and fixed ints as bigint, length-delimited fields as raw bytes); nested messages are
// decoded on demand by passing their bytes back in. Enough for the few Valve messages we read.

export type Fields = Map<number, (bigint | Buffer)[]>;

export function decode(buf: Buffer): Fields {
  const fields: Fields = new Map();
  let i = 0;
  const varint = () => {
    let result = 0n;
    for (let shift = 0n; ; shift += 7n) {
      if (i >= buf.length) throw new Error('protobuf: truncated varint');
      const byte = buf[i++];
      result |= BigInt(byte & 0x7f) << shift;
      if (!(byte & 0x80)) return result;
    }
  };
  while (i < buf.length) {
    const key = Number(varint());
    let value: bigint | Buffer;
    switch (key & 7) {
      case 0:
        value = varint();
        break;
      case 1:
        value = buf.readBigUInt64LE(i);
        i += 8;
        break;
      case 2: {
        const length = Number(varint());
        value = buf.subarray(i, i + length);
        i += length;
        break;
      }
      case 5:
        value = BigInt(buf.readUInt32LE(i));
        i += 4;
        break;
      default:
        throw new Error(`protobuf: unsupported wire type ${key & 7}`);
    }
    const list = fields.get(key >> 3);
    if (list) list.push(value);
    else fields.set(key >> 3, [value]);
  }
  return fields;
}

export function num(fields: Fields, field: number): number | undefined {
  const value = fields.get(field)?.[0];
  return typeof value === 'bigint' ? Number(value) : undefined;
}

export function bigint(fields: Fields, field: number): bigint | undefined {
  const value = fields.get(field)?.[0];
  return typeof value === 'bigint' ? value : undefined;
}

export function messages(fields: Fields, field: number): Fields[] {
  return (fields.get(field) ?? []).filter((v) => Buffer.isBuffer(v)).map(decode);
}

export function strings(fields: Fields, field: number): string[] {
  return (fields.get(field) ?? []).filter((v) => Buffer.isBuffer(v)).map((v) => v.toString('utf8'));
}
