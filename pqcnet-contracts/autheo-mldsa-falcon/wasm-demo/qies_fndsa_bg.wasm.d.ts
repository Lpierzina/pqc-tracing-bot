/* tslint:disable */
/* eslint-disable */
export const memory: WebAssembly.Memory;
export const __wbg_wasmfalconkeypair_free: (a: number, b: number) => void;
export const wasmfalconkeypair_generateFalconKeypair: (a: number) => [number, number, number];
export const wasmfalconkeypair_public_key: (a: number) => number;
export const wasmfalconkeypair_secret_key: (a: number) => number;
export const __wbg_wasmfalconpublickey_free: (a: number, b: number) => void;
export const wasmfalconpublickey_to_bytes: (a: number) => [number, number];
export const wasmfalconpublickey_get_level: (a: number) => number;
export const wasmfalconpublickey_verify: (a: number, b: number, c: number, d: number, e: number, f: number) => number;
export const wasmfalconsecretkey_sign: (a: number, b: number, c: number) => [number, number, number];
export const wasmfalconsignature_is_compressed: (a: number) => number;
export const public_key_from_bytes: (a: number, b: number, c: number) => [number, number, number];
export const self_test: (a: number) => number;
export const __wbg_wasmfalconsignature_free: (a: number, b: number) => void;
export const __wbg_wasmfalconsecretkey_free: (a: number, b: number) => void;
export const wasmfalconsignature_to_bytes: (a: number) => [number, number];
export const wasmfalconsecretkey_get_level: (a: number) => number;
export const wasmfalconsecretkey_to_bytes: (a: number) => [number, number];
export const PQCRYPTO_RUST_randombytes: (a: number, b: number) => number;
export const __wbindgen_exn_store: (a: number) => void;
export const __externref_table_alloc: () => number;
export const __wbindgen_export_2: WebAssembly.Table;
export const __externref_table_dealloc: (a: number) => void;
export const __wbindgen_free: (a: number, b: number, c: number) => void;
export const __wbindgen_malloc: (a: number, b: number) => number;
export const __wbindgen_start: () => void;
