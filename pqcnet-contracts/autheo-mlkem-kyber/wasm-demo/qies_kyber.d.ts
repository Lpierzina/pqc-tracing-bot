/* tslint:disable */
/* eslint-disable */
/**
 * Initialize panic hook for better error messages in WASM
 */
export function start(): void;
/**
 * Initialize the QIES Kyber module
 */
export function init(): void;
/**
 * Create a new KyberManager with the specified security level
 */
export function create_kyber_manager(level: number): KyberManager;
/**
 * Generate a random seed for Kyber operations
 */
export function generate_random_seed(): Uint8Array;
/**
 * Generate a random nonce for Kyber operations
 */
export function generate_random_nonce(): Uint8Array;
/**
 * Get the version of the QIES Kyber module
 */
export function get_version(): string;
/**
 * Get the name of the QIES Kyber module
 */
export function get_name(): string;
/**
 * Get the description of the QIES Kyber module
 */
export function get_description(): string;
/**
 * Security level for Kyber
 */
export enum KyberLevel {
  /**
   * ML-KEM-512 (NIST security level 1)
   */
  Level512 = 0,
  /**
   * ML-KEM-768 (NIST security level 3)
   */
  Level768 = 1,
  /**
   * ML-KEM-1024 (NIST security level 5)
   */
  Level1024 = 2,
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
  constructor(public_key: Uint8Array, private_key: Uint8Array, level: number);
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
 * Kyber Manager for handling quantum-resistant key encapsulation
 */
export class KyberManager {
  free(): void;
  /**
   * Create a new KyberManager
   */
  constructor(level: number);
  /**
   * Set the security level
   */
  set_level(level: number): void;
  /**
   * Get the current security level
   */
  get_level(): number;
  /**
   * Generate a new key pair
   */
  generate_keypair(seed: Uint8Array, nonce: Uint8Array): KyberKeyPair;
  /**
   * Encapsulate a shared secret using Kyber
   */
  encapsulate(public_key: Uint8Array, seed: Uint8Array, nonce: Uint8Array): KyberEncapsulation;
  /**
   * Decapsulate a shared secret using Kyber
   */
  decapsulate(ciphertext: Uint8Array, private_key: Uint8Array): Uint8Array;
  /**
   * Encrypt a message using Kyber KEM with AES-GCM
   */
  kyber_encrypt(message: Uint8Array, public_key: Uint8Array, seed: Uint8Array, nonce: Uint8Array): Uint8Array;
  /**
   * Decrypt a message using Kyber KEM with AES-GCM
   */
  kyber_decrypt(ciphertext: Uint8Array, private_key: Uint8Array): Uint8Array;
  /**
   * Generate random bytes using the CSPRNG
   */
  get_random_bytes(length: number): Uint8Array;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
  readonly memory: WebAssembly.Memory;
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
  readonly __wbg_kybermanager_free: (a: number, b: number) => void;
  readonly kybermanager_new: (a: number) => number;
  readonly kybermanager_set_level: (a: number, b: number) => void;
  readonly kybermanager_get_level: (a: number) => number;
  readonly kybermanager_generate_keypair: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
  readonly kybermanager_encapsulate: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => void;
  readonly kybermanager_decapsulate: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
  readonly kybermanager_kyber_encrypt: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number) => void;
  readonly kybermanager_kyber_decrypt: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
  readonly kybermanager_get_random_bytes: (a: number, b: number, c: number) => void;
  readonly start: () => void;
  readonly init: (a: number) => void;
  readonly create_kyber_manager: (a: number, b: number) => void;
  readonly generate_random_seed: (a: number) => void;
  readonly generate_random_nonce: (a: number) => void;
  readonly get_version: (a: number) => void;
  readonly get_name: (a: number) => void;
  readonly get_description: (a: number) => void;
  readonly __wbindgen_exn_store: (a: number) => void;
  readonly __wbindgen_malloc: (a: number, b: number) => number;
  readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
  readonly __wbindgen_free: (a: number, b: number, c: number) => void;
  readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
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
