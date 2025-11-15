/* tslint:disable */
/* eslint-disable */
/**
 * Build a public key object from raw bytes (used by your KAT runner)
 */
export function public_key_from_bytes(bytes: Uint8Array, level: number): WasmFalconPublicKey;
/**
 * Simple self-test: keygen → sign → verify for the given level
 */
export function self_test(level: number): boolean;
export enum WasmFalconLevel {
  /**
   * FN-DSA-512 (NIST Level 1)
   */
  Level512 = 0,
  /**
   * FN-DSA-1024 (NIST Level 5)
   */
  Level1024 = 2,
}
export class WasmFalconKeypair {
  private constructor();
  free(): void;
  /**
   * Generate a Falcon keypair (uses RNG)
   */
  static generateFalconKeypair(level: number): WasmFalconKeypair;
  public_key(): WasmFalconPublicKey;
  secret_key(): WasmFalconSecretKey;
}
export class WasmFalconPublicKey {
  private constructor();
  free(): void;
  to_bytes(): Uint8Array;
  get_level(): number;
  /**
   * Verify a signature. `_compressed` is accepted for API parity but not required.
   */
  verify(msg: Uint8Array, sig_bytes: Uint8Array, _compressed: boolean): boolean;
}
export class WasmFalconSecretKey {
  private constructor();
  free(): void;
  to_bytes(): Uint8Array;
  get_level(): number;
  /**
   * Sign a message, returning a detached signature
   */
  sign(msg: Uint8Array): WasmFalconSignature;
}
export class WasmFalconSignature {
  private constructor();
  free(): void;
  to_bytes(): Uint8Array;
  is_compressed(): boolean;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
  readonly memory: WebAssembly.Memory;
  readonly __wbg_wasmfalconkeypair_free: (a: number, b: number) => void;
  readonly wasmfalconkeypair_generateFalconKeypair: (a: number) => [number, number, number];
  readonly wasmfalconkeypair_public_key: (a: number) => number;
  readonly wasmfalconkeypair_secret_key: (a: number) => number;
  readonly __wbg_wasmfalconpublickey_free: (a: number, b: number) => void;
  readonly wasmfalconpublickey_to_bytes: (a: number) => [number, number];
  readonly wasmfalconpublickey_get_level: (a: number) => number;
  readonly wasmfalconpublickey_verify: (a: number, b: number, c: number, d: number, e: number, f: number) => number;
  readonly wasmfalconsecretkey_sign: (a: number, b: number, c: number) => [number, number, number];
  readonly wasmfalconsignature_is_compressed: (a: number) => number;
  readonly public_key_from_bytes: (a: number, b: number, c: number) => [number, number, number];
  readonly self_test: (a: number) => number;
  readonly __wbg_wasmfalconsignature_free: (a: number, b: number) => void;
  readonly __wbg_wasmfalconsecretkey_free: (a: number, b: number) => void;
  readonly wasmfalconsignature_to_bytes: (a: number) => [number, number];
  readonly wasmfalconsecretkey_get_level: (a: number) => number;
  readonly wasmfalconsecretkey_to_bytes: (a: number) => [number, number];
  readonly PQCRYPTO_RUST_randombytes: (a: number, b: number) => number;
  readonly __wbindgen_exn_store: (a: number) => void;
  readonly __externref_table_alloc: () => number;
  readonly __wbindgen_export_2: WebAssembly.Table;
  readonly __externref_table_dealloc: (a: number) => void;
  readonly __wbindgen_free: (a: number, b: number, c: number) => void;
  readonly __wbindgen_malloc: (a: number, b: number) => number;
  readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;
/**
* Instantiates the given `module`, which can either be bytes or
* a precompiled `WebAssembly.Module`.
*
* @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
*
* @returns {InitOutput}
*/
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
* If `module_or_path` is {RequestInfo} or {URL}, makes a request and
* for everything else, calls `WebAssembly.instantiate` directly.
*
* @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
*
* @returns {Promise<InitOutput>}
*/
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
