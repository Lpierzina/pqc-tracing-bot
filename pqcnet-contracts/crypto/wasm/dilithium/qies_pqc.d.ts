/* tslint:disable */
/* eslint-disable */
export function init_pqc_manager(key_storage_path: string): PQCManager;
export enum WasmDilithiumLevel {
  Level2 = 0,
  Level3 = 1,
  Level5 = 2,
}
export enum WasmKyberLevel {
  Level1 = 1,
  Level3 = 3,
  Level5 = 5,
}
/**
 * Kyber ciphertext and shared secret
 */
export class KyberEncapsulation {
  free(): void;
  /**
   * Create a new KyberEncapsulation
   */
  constructor(ciphertext: Uint8Array, shared_secret: Uint8Array);
  /**
   * Get the ciphertext
   */
  get_ciphertext(): Uint8Array;
  /**
   * Get the shared secret
   */
  get_shared_secret(): Uint8Array;
  /**
   * Serialize to JSON
   */
  to_json(): string;
  /**
   * Create from JSON
   */
  static from_json(json: string): KyberEncapsulation;
}
/**
 * Kyber key pair
 */
export class KyberKeyPair {
  free(): void;
  /**
   * Create a new KyberKeyPair
   */
  constructor(public_key: Uint8Array, private_key: Uint8Array, level_val: number);
  /**
   * Get the public key
   */
  get_public_key(): Uint8Array;
  /**
   * Get the private key
   */
  get_private_key(): Uint8Array;
  /**
   * Get the security level
   */
  get_level(): number;
  /**
   * Serialize to JSON
   */
  to_json(): string;
  /**
   * Create from JSON
   */
  static from_json(json: string): KyberKeyPair;
}
/**
 * QIES PQC Manager
 */
export class PQCManager {
  private constructor();
  free(): void;
  /**
   * Generate random bytes using our custom CSPRNG (WASM binding)
   */
  getRandomBytes(length: number): Uint8Array;
  /**
   * Generate a Dilithium key pair (WASM binding)
   */
  generateDilithiumKeypair(level_val: number): any;
  /**
   * Sign a message with Dilithium (WASM binding)
   */
  dilithiumSign(message: Uint8Array, private_key: Uint8Array, level_val: number): any;
  /**
   * Verify a Dilithium signature (WASM binding)
   */
  dilithiumVerify(message: Uint8Array, signature: Uint8Array, public_key: Uint8Array, level_val: number): boolean;
  /**
   * Generate a Kyber key pair (WASM binding)
   */
  generateKyberKeypair(level_val: number): any;
  /**
   * Encrypt a message using Kyber (WASM binding)
   */
  kyberEncrypt(message: Uint8Array, public_key: Uint8Array, level_val: number): any;
  /**
   * Decrypt a message using Kyber (WASM binding)
   */
  kyberDecrypt(ciphertext: Uint8Array, private_key: Uint8Array, level_val: number): any;
}
/**
 * WASM-compatible wrapper for Ciphertext
 */
export class WasmCiphertext {
  free(): void;
  /**
   * Create a new WasmCiphertext from bytes
   */
  constructor(bytes: Uint8Array, level: WasmKyberLevel);
  /**
   * Get the security level
   */
  get_level(): WasmKyberLevel;
  /**
   * Get the serialized bytes of this ciphertext
   */
  to_bytes(): Uint8Array;
  /**
   * Serialize to JSON
   */
  to_json(): string;
  /**
   * Create from JSON
   */
  static from_json(json: string): WasmCiphertext;
}
/**
 * WASM-compatible wrapper for DecapsulationKey
 */
export class WasmDecapsulationKey {
  free(): void;
  /**
   * Create a new WasmDecapsulationKey from bytes
   */
  constructor(bytes: Uint8Array, level: WasmKyberLevel);
  /**
   * Get the security level
   */
  get_level(): WasmKyberLevel;
  /**
   * Get the serialized bytes of this decapsulation key
   */
  to_bytes(): Uint8Array;
  /**
   * Serialize to JSON
   */
  to_json(): string;
  /**
   * Create from JSON
   */
  static from_json(json: string): WasmDecapsulationKey;
}
/**
 * WASM-compatible wrapper for EncapsulationKey
 */
export class WasmEncapsulationKey {
  free(): void;
  /**
   * Create a new WasmEncapsulationKey from bytes
   */
  constructor(bytes: Uint8Array, level: WasmKyberLevel);
  /**
   * Get the security level
   */
  get_level(): WasmKyberLevel;
  /**
   * Get the serialized bytes of this encapsulation key
   */
  to_bytes(): Uint8Array;
  /**
   * Serialize to JSON
   */
  to_json(): string;
  /**
   * Create from JSON
   */
  static from_json(json: string): WasmEncapsulationKey;
}
/**
 * WASM-compatible wrapper for Keypair
 */
export class WasmKeypair {
  free(): void;
  /**
   * Create a new keypair with the specified security level
   */
  constructor(level: WasmDilithiumLevel);
  /**
   * Generate a new keypair
   */
  static generateDilithiumKeypair(level: WasmDilithiumLevel): WasmKeypair;
  /**
   * Get the security level
   */
  get_level(): WasmDilithiumLevel;
  /**
   * Get the public key
   */
  public_key(): WasmPublicKey;
  /**
   * Get the secret key
   */
  secret_key(): WasmSecretKey;
  /**
   * Serialize to JSON
   */
  to_json(): string;
  /**
   * Create from JSON
   */
  static from_json(json: string): WasmKeypair;
}
/**
 * WASM-compatible wrapper for PublicKey
 */
export class WasmPublicKey {
  free(): void;
  /**
   * Create a new WasmPublicKey from bytes
   */
  constructor(bytes: Uint8Array, level: WasmDilithiumLevel);
  /**
   * Get the security level
   */
  get_level(): WasmDilithiumLevel;
  /**
   * Verify a signature
   */
  verify(message: Uint8Array, signature: Uint8Array, is_compressed: boolean): boolean;
  /**
   * Get the serialized bytes of this public key
   */
  to_bytes(): Uint8Array;
  /**
   * Serialize to JSON
   */
  to_json(): string;
  /**
   * Create from JSON
   */
  static from_json(json: string): WasmPublicKey;
}
/**
 * WASM-compatible wrapper for SecretKey
 */
export class WasmSecretKey {
  free(): void;
  /**
   * Create a new WasmSecretKey from bytes
   */
  constructor(bytes: Uint8Array, level: WasmDilithiumLevel);
  /**
   * Get the security level
   */
  get_level(): WasmDilithiumLevel;
  /**
   * Sign a message
   */
  sign(message: Uint8Array): WasmSignature;
  /**
   * Get the serialized bytes of this secret key
   */
  to_bytes(): Uint8Array;
  /**
   * Serialize to JSON
   */
  to_json(): string;
  /**
   * Create from JSON
   */
  static from_json(json: string): WasmSecretKey;
}
/**
 * WASM-compatible wrapper for SharedSecret
 */
export class WasmSharedSecret {
  free(): void;
  /**
   * Create a new WasmSharedSecret from bytes
   */
  constructor(bytes: Uint8Array);
  /**
   * Get the serialized bytes of this shared secret
   */
  to_bytes(): Uint8Array;
  /**
   * Serialize to JSON
   */
  to_json(): string;
  /**
   * Create from JSON
   */
  static from_json(json: string): WasmSharedSecret;
}
/**
 * WASM-compatible wrapper for Signature
 */
export class WasmSignature {
  free(): void;
  /**
   * Create a new WasmSignature from bytes
   */
  constructor(bytes: Uint8Array, level: WasmDilithiumLevel);
  /**
   * Get the security level
   */
  get_level(): WasmDilithiumLevel;
  /**
   * Check if the signature is compressed
   */
  is_compressed(): boolean;
  /**
   * Get the serialized bytes of this signature
   */
  to_bytes(): Uint8Array;
  /**
   * Serialize to JSON
   */
  to_json(): string;
  /**
   * Create from JSON
   */
  static from_json(json: string): WasmSignature;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
  readonly memory: WebAssembly.Memory;
  readonly __wbg_pqcmanager_free: (a: number, b: number) => void;
  readonly pqcmanager_getRandomBytes: (a: number, b: number, c: number) => void;
  readonly pqcmanager_generateDilithiumKeypair: (a: number, b: number, c: number) => void;
  readonly pqcmanager_dilithiumSign: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
  readonly pqcmanager_dilithiumVerify: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number) => void;
  readonly pqcmanager_generateKyberKeypair: (a: number, b: number, c: number) => void;
  readonly pqcmanager_kyberEncrypt: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
  readonly pqcmanager_kyberDecrypt: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
  readonly init_pqc_manager: (a: number, b: number, c: number) => void;
  readonly __wbg_wasmkeypair_free: (a: number, b: number) => void;
  readonly __wbg_wasmpublickey_free: (a: number, b: number) => void;
  readonly __wbg_wasmsecretkey_free: (a: number, b: number) => void;
  readonly __wbg_wasmsignature_free: (a: number, b: number) => void;
  readonly wasmpublickey_from_bytes: (a: number, b: number, c: number) => number;
  readonly wasmpublickey_get_level: (a: number) => number;
  readonly wasmpublickey_verify: (a: number, b: number, c: number, d: number, e: number, f: number) => number;
  readonly wasmpublickey_to_bytes: (a: number, b: number) => void;
  readonly wasmpublickey_to_json: (a: number, b: number) => void;
  readonly wasmpublickey_from_json: (a: number, b: number, c: number) => void;
  readonly wasmsecretkey_from_bytes: (a: number, b: number, c: number) => number;
  readonly wasmsecretkey_get_level: (a: number) => number;
  readonly wasmsecretkey_sign: (a: number, b: number, c: number) => number;
  readonly wasmsecretkey_to_bytes: (a: number, b: number) => void;
  readonly wasmsecretkey_to_json: (a: number, b: number) => void;
  readonly wasmsecretkey_from_json: (a: number, b: number, c: number) => void;
  readonly wasmsignature_from_bytes: (a: number, b: number, c: number) => number;
  readonly wasmsignature_get_level: (a: number) => number;
  readonly wasmsignature_is_compressed: (a: number) => number;
  readonly wasmsignature_to_bytes: (a: number, b: number) => void;
  readonly wasmsignature_to_json: (a: number, b: number) => void;
  readonly wasmsignature_from_json: (a: number, b: number, c: number) => void;
  readonly wasmkeypair_new: (a: number) => number;
  readonly wasmkeypair_generateDilithiumKeypair: (a: number, b: number) => void;
  readonly wasmkeypair_get_level: (a: number) => number;
  readonly wasmkeypair_public_key: (a: number) => number;
  readonly wasmkeypair_secret_key: (a: number) => number;
  readonly wasmkeypair_to_json: (a: number, b: number) => void;
  readonly wasmkeypair_from_json: (a: number, b: number, c: number) => void;
  readonly __wbg_kyberkeypair_free: (a: number, b: number) => void;
  readonly kyberkeypair_new: (a: number, b: number, c: number, d: number, e: number) => number;
  readonly kyberkeypair_get_public_key: (a: number, b: number) => void;
  readonly kyberkeypair_get_private_key: (a: number, b: number) => void;
  readonly kyberkeypair_get_level: (a: number) => number;
  readonly kyberkeypair_to_json: (a: number, b: number) => void;
  readonly kyberkeypair_from_json: (a: number, b: number, c: number) => void;
  readonly __wbg_kyberencapsulation_free: (a: number, b: number) => void;
  readonly kyberencapsulation_new: (a: number, b: number, c: number, d: number) => number;
  readonly kyberencapsulation_get_ciphertext: (a: number, b: number) => void;
  readonly kyberencapsulation_get_shared_secret: (a: number, b: number) => void;
  readonly kyberencapsulation_to_json: (a: number, b: number) => void;
  readonly kyberencapsulation_from_json: (a: number, b: number, c: number) => void;
  readonly __wbg_wasmencapsulationkey_free: (a: number, b: number) => void;
  readonly __wbg_wasmdecapsulationkey_free: (a: number, b: number) => void;
  readonly __wbg_wasmciphertext_free: (a: number, b: number) => void;
  readonly __wbg_wasmsharedsecret_free: (a: number, b: number) => void;
  readonly wasmencapsulationkey_from_bytes: (a: number, b: number, c: number) => number;
  readonly wasmencapsulationkey_get_level: (a: number) => number;
  readonly wasmencapsulationkey_to_bytes: (a: number, b: number) => void;
  readonly wasmencapsulationkey_to_json: (a: number, b: number) => void;
  readonly wasmencapsulationkey_from_json: (a: number, b: number, c: number) => void;
  readonly wasmdecapsulationkey_from_bytes: (a: number, b: number, c: number) => number;
  readonly wasmdecapsulationkey_get_level: (a: number) => number;
  readonly wasmdecapsulationkey_to_bytes: (a: number, b: number) => void;
  readonly wasmdecapsulationkey_to_json: (a: number, b: number) => void;
  readonly wasmdecapsulationkey_from_json: (a: number, b: number, c: number) => void;
  readonly wasmciphertext_from_bytes: (a: number, b: number, c: number) => number;
  readonly wasmciphertext_get_level: (a: number) => number;
  readonly wasmciphertext_to_bytes: (a: number, b: number) => void;
  readonly wasmciphertext_to_json: (a: number, b: number) => void;
  readonly wasmciphertext_from_json: (a: number, b: number, c: number) => void;
  readonly wasmsharedsecret_from_bytes: (a: number, b: number) => number;
  readonly wasmsharedsecret_to_bytes: (a: number, b: number) => void;
  readonly wasmsharedsecret_to_json: (a: number, b: number) => void;
  readonly wasmsharedsecret_from_json: (a: number, b: number, c: number) => void;
  readonly rust_zstd_wasm_shim_qsort: (a: number, b: number, c: number, d: number) => void;
  readonly rust_zstd_wasm_shim_malloc: (a: number) => number;
  readonly rust_zstd_wasm_shim_memcmp: (a: number, b: number, c: number) => number;
  readonly rust_zstd_wasm_shim_calloc: (a: number, b: number) => number;
  readonly rust_zstd_wasm_shim_free: (a: number) => void;
  readonly rust_zstd_wasm_shim_memcpy: (a: number, b: number, c: number) => number;
  readonly rust_zstd_wasm_shim_memmove: (a: number, b: number, c: number) => number;
  readonly rust_zstd_wasm_shim_memset: (a: number, b: number, c: number) => number;
  readonly __wbindgen_malloc: (a: number, b: number) => number;
  readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
  readonly __wbindgen_exn_store: (a: number) => void;
  readonly __wbindgen_free: (a: number, b: number, c: number) => void;
  readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
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
