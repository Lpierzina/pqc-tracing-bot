package main

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"flag"
	"fmt"
	"log"
	"os"
	"time"

	"github.com/tetratelabs/wazero"
	"github.com/tetratelabs/wazero/api"
	"golang.org/x/crypto/blake2s"
)

const (
	handshakeHeaderLen = 4 + 1 + 1 + 1 + 1 + 1 + 1 + 32 + 32 + 8 + 8 + (2 * 5)
	transcriptDomain   = "PQCNET_MLDSA_SIG_V1"
)

func main() {
	defaultWasm := "../pqcnet-contracts/target/wasm32-unknown-unknown/release/autheo_pqc_wasm.wasm"
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
	dag := newDagHost()
	dag.registerPayload(request.edgeID, request.bytes)
	fmt.Printf("Handshake request (%d bytes, edge=%s): %q\n", len(request.bytes), request.edgeID, request.bytes)

	reqPtr := mustAllocAndWrite(ctx, module, allocFn, request.bytes)

	const respLen = 4096
	respPtr := mustAlloc(ctx, module, allocFn, respLen)

	written := callHandshake(ctx, handshakeFn, reqPtr, len(request.bytes), respPtr, respLen)
	rawResponse := readFromMemory(module, respPtr, written)

	freeBuffer(ctx, freeFn, reqPtr, len(request.bytes))
	freeBuffer(ctx, freeFn, respPtr, respLen)

	envelope, err := parseHandshakeResponse(rawResponse)
	if err != nil {
		log.Fatalf("parse handshake response: %v", err)
	}

	registry := newKeyRegistry()
	registry.persist(envelope)

	fmt.Printf(
		"Handshake OK → kem_key=%s signer=%s t=%d/%d ciphertext=%dB shared=%dB signature=%dB\n",
		envelope.KemKeyID.Hex(),
		envelope.SigningKeyID.Hex(),
		envelope.Threshold.T,
		envelope.Threshold.N,
		len(envelope.Ciphertext),
		len(envelope.SharedSecret),
		len(envelope.Signature),
	)

	if err := verifyTranscript(envelope, request.bytes); err != nil {
		log.Fatalf("transcript verification failed: %v", err)
	}

	if err := dag.verifyAndAnchor(
		request.edgeID,
		envelope.SigningKeyID,
		envelope.Signature,
		func(payload []byte) error {
			return verifyTranscript(envelope, payload)
		},
	); err != nil {
		log.Fatalf("qs-dag anchoring failed: %v", err)
	}

	fmt.Printf("QS-DAG anchor stored for edge=%s signer=%s\n", request.edgeID, envelope.SigningKeyID.Hex())
}

type handshakeInput struct {
	bytes  []byte
	edgeID string
}

func buildRequestPayload() handshakeInput {
	now := time.Now().UTC()
	payload := fmt.Sprintf("client=autheo-demo&ts=%d", now.UnixNano())
	data := []byte(payload)
	return handshakeInput{
		bytes:  data,
		edgeID: deriveEdgeID(data),
	}
}

func deriveEdgeID(payload []byte) string {
	sum := sha256.Sum256(payload)
	return hex.EncodeToString(sum[:])
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
	result := make([]byte, length)
	copy(result, data)
	return result
}

type keyID [32]byte

func (k keyID) Hex() string {
	return hex.EncodeToString(k[:])
}

type ThresholdPolicy struct {
	T uint8
	N uint8
}

type handshakeResponse struct {
	Version      uint8
	KemLevel     uint8
	DsaLevel     uint8
	Threshold    ThresholdPolicy
	KemKeyID     keyID
	SigningKeyID keyID
	KemCreatedAt uint64
	KemExpiresAt uint64
	Ciphertext   []byte
	SharedSecret []byte
	Signature    []byte
	KemPublicKey []byte
	DsaPublicKey []byte
}

func parseHandshakeResponse(data []byte) (*handshakeResponse, error) {
	if len(data) < handshakeHeaderLen {
		return nil, fmt.Errorf("handshake response too short: %d bytes", len(data))
	}
	if !bytes.Equal(data[:4], []byte("PQC1")) {
		return nil, fmt.Errorf("handshake magic mismatch: %x", data[:4])
	}

	header := data[:handshakeHeaderLen]
	cursor := 4

	resp := &handshakeResponse{}
	resp.Version = header[cursor]
	cursor++
	resp.KemLevel = header[cursor]
	cursor++
	resp.DsaLevel = header[cursor]
	cursor++
	resp.Threshold.T = header[cursor]
	cursor++
	resp.Threshold.N = header[cursor]
	cursor++
	cursor++ // reserved

	copy(resp.KemKeyID[:], header[cursor:cursor+32])
	cursor += 32
	copy(resp.SigningKeyID[:], header[cursor:cursor+32])
	cursor += 32

	resp.KemCreatedAt = binary.LittleEndian.Uint64(header[cursor : cursor+8])
	cursor += 8
	resp.KemExpiresAt = binary.LittleEndian.Uint64(header[cursor : cursor+8])
	cursor += 8

	cipherLen := binary.LittleEndian.Uint16(header[cursor : cursor+2])
	cursor += 2
	secretLen := binary.LittleEndian.Uint16(header[cursor : cursor+2])
	cursor += 2
	sigLen := binary.LittleEndian.Uint16(header[cursor : cursor+2])
	cursor += 2
	kemPkLen := binary.LittleEndian.Uint16(header[cursor : cursor+2])
	cursor += 2
	dsaPkLen := binary.LittleEndian.Uint16(header[cursor : cursor+2])
	cursor += 2

	expected := handshakeHeaderLen +
		int(cipherLen) +
		int(secretLen) +
		int(sigLen) +
		int(kemPkLen) +
		int(dsaPkLen)
	if len(data) != expected {
		return nil, fmt.Errorf("handshake length mismatch: got %d want %d", len(data), expected)
	}

	offset := handshakeHeaderLen
	resp.Ciphertext = copySection(data, offset, int(cipherLen))
	offset += int(cipherLen)
	resp.SharedSecret = copySection(data, offset, int(secretLen))
	offset += int(secretLen)
	resp.Signature = copySection(data, offset, int(sigLen))
	offset += int(sigLen)
	resp.KemPublicKey = copySection(data, offset, int(kemPkLen))
	offset += int(kemPkLen)
	resp.DsaPublicKey = copySection(data, offset, int(dsaPkLen))

	return resp, nil
}

func copySection(src []byte, offset, length int) []byte {
	section := make([]byte, length)
	copy(section, src[offset:offset+length])
	return section
}

type keyMetadata struct {
	KeyID     string
	Level     uint8
	CreatedAt uint64
	ExpiresAt uint64
	Threshold ThresholdPolicy
	PublicKey []byte
}

type keyRegistry struct {
	kem map[string]keyMetadata
	dsa map[string]keyMetadata
}

func newKeyRegistry() *keyRegistry {
	return &keyRegistry{
		kem: make(map[string]keyMetadata),
		dsa: make(map[string]keyMetadata),
	}
}

func (r *keyRegistry) persist(resp *handshakeResponse) {
	kemKey := resp.KemKeyID.Hex()
	r.kem[kemKey] = keyMetadata{
		KeyID:     kemKey,
		Level:     resp.KemLevel,
		CreatedAt: resp.KemCreatedAt,
		ExpiresAt: resp.KemExpiresAt,
		Threshold: resp.Threshold,
		PublicKey: append([]byte(nil), resp.KemPublicKey...),
	}

	dsaKey := resp.SigningKeyID.Hex()
	r.dsa[dsaKey] = keyMetadata{
		KeyID:     dsaKey,
		Level:     resp.DsaLevel,
		CreatedAt: resp.KemCreatedAt,
		ExpiresAt: resp.KemExpiresAt,
		Threshold: resp.Threshold,
		PublicKey: append([]byte(nil), resp.DsaPublicKey...),
	}
}

func verifyTranscript(resp *handshakeResponse, payload []byte) error {
	transcript := make([]byte, 0, len(resp.Ciphertext)+len(resp.SharedSecret)+len(payload))
	transcript = append(transcript, resp.Ciphertext...)
	transcript = append(transcript, resp.SharedSecret...)
	transcript = append(transcript, payload...)

	digest, err := blake2s.New256(nil)
	if err != nil {
		return fmt.Errorf("blake2s init: %w", err)
	}
	digest.Write([]byte(transcriptDomain))
	digest.Write(resp.DsaPublicKey)
	digest.Write(transcript)
	expected := digest.Sum(nil)

	if !bytes.Equal(expected, resp.Signature) {
		return fmt.Errorf("signature mismatch")
	}
	return nil
}

type dagHost struct {
	payloads map[string][]byte
	anchors  map[string][][]byte
}

func newDagHost() *dagHost {
	return &dagHost{
		payloads: make(map[string][]byte),
		anchors:  make(map[string][][]byte),
	}
}

func (d *dagHost) registerPayload(edgeID string, payload []byte) {
	d.payloads[edgeID] = append([]byte(nil), payload...)
}

func (d *dagHost) verifyAndAnchor(edgeID string, signer keyID, signature []byte, verifyFn func(payload []byte) error) error {
	payload, ok := d.payloads[edgeID]
	if !ok {
		return fmt.Errorf("edge %s payload missing", edgeID)
	}
	if err := verifyFn(payload); err != nil {
		return err
	}

	key := fmt.Sprintf("%s::%s", edgeID, signer.Hex())
	d.anchors[key] = append(d.anchors[key], append([]byte(nil), signature...))
	return nil
}
