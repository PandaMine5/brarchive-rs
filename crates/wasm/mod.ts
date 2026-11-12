/**
 * Read and write Bedrock Archive (`.brarchive`) files, the bundling format
 * Mojang uses to pack the files inside Minecraft Bedrock Edition resource and
 * behavior packs.
 *
 * The format is implemented in Rust and compiled to WebAssembly. The wasm
 * module is loaded when this module is first imported, so every function here
 * is synchronous.
 *
 * ```ts
 * import { deserialize, list, serialize } from "@bedrock-crustaceans/brarchive";
 *
 * const bytes = serialize({
 *   "zombie.entity.json": '{"format_version":"1.10.0"}',
 *   "zombie.mcb": new Uint8Array([0x7f, 0x4d, 0x43, 0x42]),
 * });
 *
 * list(bytes); // ["zombie.entity.json", "zombie.mcb"]
 *
 * const entries = deserialize(bytes);
 * new TextDecoder().decode(entries.get("zombie.entity.json"));
 * ```
 *
 * @module
 */

import * as wasm from "./lib/brarchive_wasm.js";

/**
 * Content of one archive entry. Strings are encoded as UTF-8; byte arrays are
 * stored as-is, which is how binary entries such as Mojang's compiled MCB
 * files are represented.
 */
export type EntryContent = Uint8Array | string;

/**
 * Anything that yields `[name, content]` pairs, such as a `Map`, an array of
 * tuples, or a plain object keyed by entry name.
 */
export type Entries =
  | Iterable<readonly [string, EntryContent]>
  | Readonly<Record<string, EntryContent>>;

/** Options accepted by {@link serialize}. */
export interface SerializeOptions {
  /**
   * Store identical content only once. Entries with the same bytes share a
   * single block in the archive, which shrinks packs with duplicated files.
   * Defaults to `false`.
   */
  dedup?: boolean;
}

const encoder = new TextEncoder();

function toPairs(entries: Entries): Array<[string, Uint8Array]> {
  const iterable: Iterable<readonly [string, EntryContent]> =
    typeof (entries as Iterable<unknown>)[Symbol.iterator] === "function"
      ? entries as Iterable<readonly [string, EntryContent]>
      : Object.entries(entries as Record<string, EntryContent>);

  const pairs: Array<[string, Uint8Array]> = [];
  for (const [name, content] of iterable) {
    if (typeof name !== "string") {
      throw new TypeError("entry names must be strings");
    }
    if (typeof content === "string") {
      pairs.push([name, encoder.encode(content)]);
    } else if (content instanceof Uint8Array) {
      pairs.push([name, content]);
    } else {
      throw new TypeError(
        `entry "${name}" must be a string or Uint8Array`,
      );
    }
  }
  return pairs;
}

/**
 * Encode entries into `.brarchive` bytes.
 *
 * Entries are written in the order given. Names must be unique; an archive
 * with duplicate names is rejected by the format.
 *
 * @throws {Error} if an entry name is too long for the format or the input
 * cannot be encoded.
 */
export function serialize(
  entries: Entries,
  options: SerializeOptions = {},
): Uint8Array {
  const pairs = toPairs(entries);
  const names = pairs.map(([name]) => name);
  const lengths = new Uint32Array(pairs.length);
  let total = 0;
  pairs.forEach(([, content], i) => {
    lengths[i] = content.byteLength;
    total += content.byteLength;
  });
  const data = new Uint8Array(total);
  let offset = 0;
  for (const [, content] of pairs) {
    data.set(content, offset);
    offset += content.byteLength;
  }
  return wasm.serialize(names, data, lengths, options.dedup ?? false);
}

/**
 * Decode `.brarchive` bytes into a map from entry name to content.
 *
 * Content comes back as raw bytes because an archive can hold binary entries.
 * Use {@link deserializeText} when you know every entry is text.
 *
 * @throws {Error} if the bytes are not a valid archive (bad magic, unsupported
 * version, truncated data).
 */
export function deserialize(bytes: Uint8Array): Map<string, Uint8Array> {
  const raw = wasm.deserialize(bytes);
  try {
    const { names, data, lengths } = raw;
    const result = new Map<string, Uint8Array>();
    let offset = 0;
    for (let i = 0; i < names.length; i++) {
      const end = offset + lengths[i];
      result.set(names[i], data.slice(offset, end));
      offset = end;
    }
    return result;
  } finally {
    raw.free();
  }
}

/**
 * Decode `.brarchive` bytes into a map from entry name to UTF-8 text.
 *
 * Convenient for archives that only hold JSON, which is the common case.
 *
 * @throws {TypeError} if an entry is not valid UTF-8, for example a compiled
 * binary MCB entry. Use {@link deserialize} for those archives.
 */
export function deserializeText(bytes: Uint8Array): Map<string, string> {
  const decoder = new TextDecoder("utf-8", { fatal: true });
  const result = new Map<string, string>();
  for (const [name, content] of deserialize(bytes)) {
    result.set(name, decoder.decode(content));
  }
  return result;
}

/**
 * List the entry names in `.brarchive` bytes without decoding any content.
 *
 * @throws {Error} if the bytes are not a valid archive.
 */
export function list(bytes: Uint8Array): string[] {
  return wasm.list(bytes);
}
