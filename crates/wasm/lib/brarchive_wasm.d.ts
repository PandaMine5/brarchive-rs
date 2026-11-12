// @generated file from wasmbuild -- do not edit
// deno-lint-ignore-file
// deno-fmt-ignore-file

/**
 * A decoded archive in flat form: `names[i]` owns the `lengths[i]` bytes of
 * `data` that follow the previous entry's bytes.
 */
export class RawEntries {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  data: Uint8Array;
  lengths: Uint32Array;
  names: string[];
}

/**
 * Decode `.brarchive` bytes into flat entries. See [`RawEntries`].
 */
export function deserialize(data: Uint8Array): RawEntries;

/**
 * List entry names without decoding any content.
 */
export function list(data: Uint8Array): string[];

/**
 * Encode entries into `.brarchive` bytes. `data` holds every entry's content
 * back to back and `lengths[i]` is the byte length of `names[i]`.
 */
export function serialize(
  names: string[],
  data: Uint8Array,
  lengths: Uint32Array,
  dedup: boolean,
): Uint8Array;
