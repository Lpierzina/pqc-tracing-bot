package main

import (
	"context"
	"encoding/hex"
	"flag"
	"fmt"
	"log"
	"os"
	"time"

	"github.com/tetratelabs/wazero"
	"github.com/tetratelabs/wazero/api"
)

func main() {
	defaultWasm := "../pqcnet-contracts/target/wasm32-unknown-unknown/release/pqcnet_contracts.wasm"
	wasmPath := flag.String("wasm", defaultWasm, "path to the built pqcnet WASM module")
	flag.Parse()

	wasmBytes, err := os.ReadFile(*wasmPath)
	if err != nil {
		log.Fatalf("failed to read WASM module %q: %v", *wasmPath, err)
	}

	ctx := context.Background()
	runtime := wazero.NewRuntime(ctx)
	defer runtime.Close(ctx)

	compiled, err := runtime.CompileModule(ctx, wasmBytes)
	if err != nil {
		log.Fatalf("compile module: %v", err)
	}
	defer compiled.Close(ctx)

	module, err := runtime.InstantiateModule(ctx, compiled, wazero.NewModuleConfig().WithName("pqcnet_enclave"))
	if err != nil {
		log.Fatalf("instantiate module: %v", err)
	}
	defer module.Close(ctx)

	allocFn := exportedFunction(module, "pqc_alloc")
	freeFn := exportedFunction(module, "pqc_free")
	handshakeFn := exportedFunction(module, "pqc_handshake")

	request := buildRequestPayload()
	reqPtr := mustAllocAndWrite(ctx, module, allocFn, request)

	const respLen = 64
	respPtr := mustAlloc(ctx, module, allocFn, respLen)

	written := callHandshake(ctx, handshakeFn, reqPtr, len(request), respPtr, respLen)
	response := readFromMemory(module, respPtr, written)

	freeBuffer(ctx, freeFn, reqPtr, len(request))
	freeBuffer(ctx, freeFn, respPtr, respLen)

	fmt.Printf("Handshake request (%d bytes): %q\n", len(request), request)
	fmt.Printf("Handshake response (%d bytes): %s\n", written, hex.EncodeToString(response))
}

func buildRequestPayload() []byte {
	now := time.Now().UTC()
	payload := fmt.Sprintf("client=autheo-demo&ts=%d", now.UnixNano())
	return []byte(payload)
}

func exportedFunction(module api.Module, name string) api.Function {
	fn := module.ExportedFunction(name)
	if fn == nil {
		log.Fatalf("WASM export %q is missing", name)
	}
	return fn
}

func mustAlloc(ctx context.Context, module api.Module, alloc api.Function, size int) uint32 {
	if size <= 0 {
		log.Fatalf("allocation size must be positive, got %d", size)
	}

	results, err := alloc.Call(ctx, uint64(size))
	if err != nil {
		log.Fatalf("pqc_alloc(%d) failed: %v", size, err)
	}
	if len(results) != 1 {
		log.Fatalf("pqc_alloc returned unexpected result count: %d", len(results))
	}
	ptr := uint32(results[0])
	if ptr == 0 {
		log.Fatalf("pqc_alloc returned null pointer for %d bytes", size)
	}
	return ptr
}

func mustAllocAndWrite(ctx context.Context, module api.Module, alloc api.Function, data []byte) uint32 {
	ptr := mustAlloc(ctx, module, alloc, len(data))
	if ok := module.Memory().Write(ptr, data); !ok {
		log.Fatalf("failed to copy %d bytes into WASM memory", len(data))
	}
	return ptr
}

func freeBuffer(ctx context.Context, free api.Function, ptr uint32, size int) {
	if ptr == 0 || size == 0 {
		return
	}
	if _, err := free.Call(ctx, uint64(ptr), uint64(size)); err != nil {
		log.Printf("warning: pqc_free(%d, %d) failed: %v", ptr, size, err)
	}
}

func callHandshake(ctx context.Context, handshake api.Function, reqPtr uint32, reqLen int, respPtr uint32, respLen int) int {
	results, err := handshake.Call(
		ctx,
		uint64(reqPtr),
		uint64(reqLen),
		uint64(respPtr),
		uint64(respLen),
	)
	if err != nil {
		log.Fatalf("pqc_handshake call failed: %v", err)
	}
	if len(results) != 1 {
		log.Fatalf("pqc_handshake returned unexpected result count: %d", len(results))
	}

	written := int32(uint32(results[0]))
	if written < 0 {
		switch written {
		case -1:
			log.Fatalf("pqc_handshake reported invalid input (request len %d)", reqLen)
		case -2:
			log.Fatalf("pqc_handshake reported response buffer too small (len %d)", respLen)
		default:
			log.Fatalf("pqc_handshake reported internal error (code %d)", written)
		}
	}
	if int(written) > respLen {
		log.Fatalf("pqc_handshake wrote %d bytes but buffer is %d", written, respLen)
	}
	return int(written)
}

func readFromMemory(module api.Module, ptr uint32, length int) []byte {
	data, ok := module.Memory().Read(ptr, uint32(length))
	if !ok {
		log.Fatalf("failed to read %d bytes from WASM memory @ %d", length, ptr)
	}
	// Copy into a Go-owned slice to avoid referencing WASM memory after it could be freed.
	result := make([]byte, length)
	copy(result, data)
	return result
}
