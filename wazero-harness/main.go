package main

import (
	"bytes"
	"context"
	"crypto/rand"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"log"
	"os"
	"path/filepath"
	"strings"
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
	defaultEntropy := "../pqcnet-contracts/target/wasm32-unknown-unknown/release/autheo_entropy_wasm.wasm"
	entropyPath := flag.String("entropy", defaultEntropy, "path to the built entropy WASM module")
	qrngBridge := flag.String("qrng-bridge", "../pqcnet-contracts/target/chsh_bridge_state.json", "path to the QRNG bridge snapshot JSON")
	qrngResults := flag.String("qrng-results", "../pqcnet-contracts/target/chsh_results.json", "optional path to the CHSH sandbox results JSON (empty to skip)")
	qrngSource := flag.String("qrng-source", "sandbox", "label for the QRNG source (e.g., sandbox or hardware:rpi-alpha)")
	abw34Log := flag.String("abw34-log", "../pqcnet-contracts/target/abw34_log.jsonl", "file to append ABW34 telemetry (empty to disable)")
	shardCount := flag.Int("shards", 10, "active shard count for the current run")
	noiseRatio := flag.Float64("noise-ratio", 0.0, "synthetic noise ratio injected into the load generator (0.0-1.0)")
	qaceReroutes := flag.Int("qace-reroutes", 0, "number of QACE reroutes observed during the run")
	perShardTps := flag.Float64("tps-per-shard", 1_500_000, "observed TPS per shard under current conditions")
	flag.Parse()

	wasmBytes, err := os.ReadFile(*wasmPath)
	if err != nil {
		log.Fatalf("failed to read WASM module %q: %v", *wasmPath, err)
	}
	entropyBytes, err := os.ReadFile(*entropyPath)
	if err != nil {
		log.Fatalf("failed to read entropy WASM module %q: %v", *entropyPath, err)
	}

	ctx := context.Background()
	runtime := wazero.NewRuntime(ctx)
	defer runtime.Close(ctx)

	entropyCompiled, err := runtime.CompileModule(ctx, entropyBytes)
	if err != nil {
		log.Fatalf("compile entropy module: %v", err)
	}
	defer entropyCompiled.Close(ctx)

	entropyModule, err := runtime.InstantiateModule(ctx, entropyCompiled, wazero.NewModuleConfig().WithName("autheo_entropy"))
	if err != nil {
		log.Fatalf("instantiate entropy module: %v", err)
	}
	defer entropyModule.Close(ctx)

	entropyNode := newEntropyNode(entropyModule)
	qrngSample, err := loadQrngSample(*qrngBridge, *qrngResults, *qrngSource)
	if err != nil {
		log.Fatalf("load QRNG feed: %v", err)
	}
	if err := entropyNode.seedWithBytes(ctx, qrngSample.Seed); err != nil {
		log.Fatalf("seed entropy node: %v", err)
	}
	fmt.Printf("QRNG feed loaded → source=%s epoch=%d tuple=%s shard=%d seed=%s bits=%d\n",
		qrngSample.Source, qrngSample.Epoch, qrngSample.TupleID, qrngSample.ShardID, qrngSample.SeedHex, qrngSample.Bits)
	if qrngSample.TwoQubitExact > 0 && qrngSample.FiveDExact > 0 {
		fmt.Printf("  CHSH two-qubit exact=%.4f sampled=%.4f · 5D exact=%.4f sampled=%.4f shots=%d depolarizing=%.3f\n",
			qrngSample.TwoQubitExact, qrngSample.TwoQubitSampled,
			qrngSample.FiveDExact, qrngSample.FiveDSampled,
			qrngSample.Shots, qrngSample.Depolarizing)
	}
	if err := entropyNode.ensureHealthy(ctx); err != nil {
		log.Fatalf("entropy node health check failed: %v", err)
	}

	hostModule, err := registerHostEntropy(ctx, runtime, entropyNode)
	if err != nil {
		log.Fatalf("register host entropy: %v", err)
	}
	defer hostModule.Close(ctx)

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

	if err := appendAbw34Record(*abw34Log, abw34Record{
		TimestampMs:         time.Now().UnixMilli(),
		QrngSource:          qrngSample.Source,
		QrngEpoch:           qrngSample.Epoch,
		QrngTupleID:         qrngSample.TupleID,
		QrngSeedHex:         qrngSample.SeedHex,
		QrngBits:            qrngSample.Bits,
		QrngShardID:         qrngSample.ShardID,
		ShardCount:          *shardCount,
		NoiseRatio:          clamp01(*noiseRatio),
		QaceReroutes:        *qaceReroutes,
		ObservedTpsShard:    *perShardTps,
		ObservedTpsGlobal:   *perShardTps * float64(*shardCount),
		KemKeyID:            envelope.KemKeyID.Hex(),
		SigningKeyID:        envelope.SigningKeyID.Hex(),
		KemCreatedAt:        envelope.KemCreatedAt,
		KemExpiresAt:        envelope.KemExpiresAt,
		ChshTwoQubitExact:   qrngSample.TwoQubitExact,
		ChshTwoQubitSampled: qrngSample.TwoQubitSampled,
		ChshFiveDExact:      qrngSample.FiveDExact,
		ChshFiveDSampled:    qrngSample.FiveDSampled,
		ChshShots:           qrngSample.Shots,
		ChshDepolarizing:    qrngSample.Depolarizing,
	}); err != nil {
		log.Fatalf("record ABW34 telemetry: %v", err)
	}
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

type qrngSample struct {
	Source          string
	Epoch           uint64
	Seed            []byte
	SeedHex         string
	TupleID         string
	ShardID         uint16
	Bits            int
	HyperBits       int
	TwoQubitExact   float64
	TwoQubitSampled float64
	FiveDExact      float64
	FiveDSampled    float64
	Shots           int
	Depolarizing    float64
}

type bridgeSnapshot struct {
	QrngEpoch      uint64       `json:"qrng_epoch"`
	QrngSeedHex    string       `json:"qrng_seed_hex"`
	QrngBits       int          `json:"qrng_bits"`
	HyperTupleBits int          `json:"hyper_tuple_bits"`
	TupleReceipt   tupleReceipt `json:"tuple_receipt"`
}

type tupleReceipt struct {
	TupleID string `json:"tuple_id"`
	ShardID uint16 `json:"shard_id"`
}

type chshResultsFile struct {
	TwoQubit     *chshSection `json:"two_qubit"`
	FiveD        *chshSection `json:"five_d"`
	Shots        int          `json:"shots"`
	Depolarizing float64      `json:"depolarizing"`
}

type chshSection struct {
	Exact   float64 `json:"exact"`
	Sampled float64 `json:"sampled"`
}

func loadQrngSample(bridgePath, resultsPath, source string) (*qrngSample, error) {
	data, err := os.ReadFile(bridgePath)
	if err != nil {
		return nil, err
	}
	var snapshot bridgeSnapshot
	if err := json.Unmarshal(data, &snapshot); err != nil {
		return nil, err
	}
	trimmed := strings.TrimSpace(snapshot.QrngSeedHex)
	if len(trimmed) < 64 {
		return nil, fmt.Errorf("qrng seed hex too short: %s", trimmed)
	}
	seedHex := trimmed[:64]
	seed, err := hex.DecodeString(seedHex)
	if err != nil {
		return nil, fmt.Errorf("decode seed hex: %w", err)
	}
	sample := &qrngSample{
		Source:    source,
		Epoch:     snapshot.QrngEpoch,
		Seed:      seed,
		SeedHex:   seedHex,
		TupleID:   snapshot.TupleReceipt.TupleID,
		ShardID:   snapshot.TupleReceipt.ShardID,
		Bits:      snapshot.QrngBits,
		HyperBits: snapshot.HyperTupleBits,
	}

	if resultsPath != "" {
		if err := applyChshResults(resultsPath, sample); err != nil {
			return nil, err
		}
	}

	return sample, nil
}

func applyChshResults(path string, sample *qrngSample) error {
	data, err := os.ReadFile(path)
	if err != nil {
		if errors.Is(err, os.ErrNotExist) {
			return nil
		}
		return err
	}
	var file chshResultsFile
	if err := json.Unmarshal(data, &file); err != nil {
		return err
	}
	if file.TwoQubit != nil {
		sample.TwoQubitExact = file.TwoQubit.Exact
		sample.TwoQubitSampled = file.TwoQubit.Sampled
	}
	if file.FiveD != nil {
		sample.FiveDExact = file.FiveD.Exact
		sample.FiveDSampled = file.FiveD.Sampled
	}
	sample.Shots = file.Shots
	sample.Depolarizing = file.Depolarizing
	return nil
}

type abw34Record struct {
	TimestampMs         int64   `json:"timestamp_ms"`
	QrngSource          string  `json:"qrng_source"`
	QrngEpoch           uint64  `json:"qrng_epoch"`
	QrngTupleID         string  `json:"qrng_tuple_id"`
	QrngSeedHex         string  `json:"qrng_seed_hex"`
	QrngBits            int     `json:"qrng_bits"`
	QrngShardID         uint16  `json:"qrng_shard_id"`
	ShardCount          int     `json:"shard_count"`
	NoiseRatio          float64 `json:"noise_ratio"`
	QaceReroutes        int     `json:"qace_reroutes"`
	ObservedTpsShard    float64 `json:"observed_tps_per_shard"`
	ObservedTpsGlobal   float64 `json:"observed_tps_global"`
	KemKeyID            string  `json:"kem_key_id"`
	SigningKeyID        string  `json:"signing_key_id"`
	KemCreatedAt        uint64  `json:"kem_created_at"`
	KemExpiresAt        uint64  `json:"kem_expires_at"`
	ChshTwoQubitExact   float64 `json:"chsh_two_qubit_exact,omitempty"`
	ChshTwoQubitSampled float64 `json:"chsh_two_qubit_sampled,omitempty"`
	ChshFiveDExact      float64 `json:"chsh_five_d_exact,omitempty"`
	ChshFiveDSampled    float64 `json:"chsh_five_d_sampled,omitempty"`
	ChshShots           int     `json:"chsh_shots,omitempty"`
	ChshDepolarizing    float64 `json:"chsh_depolarizing,omitempty"`
}

func appendAbw34Record(path string, record abw34Record) error {
	if path == "" {
		return nil
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	f, err := os.OpenFile(path, os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0o644)
	if err != nil {
		return err
	}
	defer f.Close()
	enc := json.NewEncoder(f)
	enc.SetEscapeHTML(false)
	return enc.Encode(record)
}

func clamp01(v float64) float64 {
	if v < 0 {
		return 0
	}
	if v > 1 {
		return 1
	}
	return v
}

func registerHostEntropy(ctx context.Context, runtime wazero.Runtime, node *entropyNode) (api.Module, error) {
	builder := runtime.NewHostModuleBuilder("autheo")
	builder.NewFunctionBuilder().
		WithParameterNames("ptr", "len").
		WithResultNames("errno").
		WithGoModuleFunction(api.GoModuleFunc(func(ctx context.Context, caller api.Module, stack []uint64) {
			ptr := api.DecodeU32(stack[0])
			length := api.DecodeU32(stack[1])
			status := node.bridge(ctx, caller, ptr, length)
			stack[0] = api.EncodeI32(status)
		}), []api.ValueType{api.ValueTypeI32, api.ValueTypeI32}, []api.ValueType{api.ValueTypeI32}).
		Export("autheo_host_entropy")
	return builder.Instantiate(ctx)
}

type entropyNode struct {
	module api.Module
	alloc  api.Function
	free   api.Function
	seed   api.Function
	fill   api.Function
	health api.Function
}

func newEntropyNode(module api.Module) *entropyNode {
	return &entropyNode{
		module: module,
		alloc:  exportedFunction(module, "autheo_entropy_alloc"),
		free:   exportedFunction(module, "autheo_entropy_free"),
		seed:   exportedFunction(module, "autheo_entropy_seed"),
		fill:   exportedFunction(module, "autheo_entropy_fill"),
		health: exportedFunction(module, "autheo_entropy_health"),
	}
}

func (n *entropyNode) seedWithRandom(ctx context.Context) error {
	seed := make([]byte, 32)
	if _, err := rand.Read(seed); err != nil {
		return fmt.Errorf("secure random seed: %w", err)
	}
	return n.seedWithBytes(ctx, seed)
}

func (n *entropyNode) seedWithBytes(ctx context.Context, seed []byte) error {
	if len(seed) == 0 {
		return fmt.Errorf("entropy seed cannot be empty")
	}
	ptr, err := n.allocBuffer(ctx, uint32(len(seed)))
	if err != nil {
		return fmt.Errorf("entropy alloc for seed: %w", err)
	}
	defer n.freeBuffer(ctx, ptr, uint32(len(seed)))
	if !n.module.Memory().Write(ptr, seed) {
		return fmt.Errorf("entropy seed write failed")
	}
	results, err := n.seed.Call(ctx, uint64(ptr), uint64(len(seed)))
	if err != nil {
		return fmt.Errorf("entropy seed call failed: %w", err)
	}
	if len(results) == 0 {
		return fmt.Errorf("entropy seed returned no status")
	}
	status := int32(uint32(results[0]))
	if status != 0 {
		return fmt.Errorf("entropy seed rejected with code %d", status)
	}
	return nil
}

func (n *entropyNode) ensureHealthy(ctx context.Context) error {
	results, err := n.health.Call(ctx)
	if err != nil {
		return fmt.Errorf("entropy health call failed: %w", err)
	}
	if len(results) == 0 {
		return fmt.Errorf("entropy health returned no status")
	}
	status := int32(uint32(results[0]))
	if status != 0 {
		return fmt.Errorf("entropy node unhealthy (code %d)", status)
	}
	return nil
}

func (n *entropyNode) bridge(ctx context.Context, consumer api.Module, destPtr, length uint32) int32 {
	if length == 0 {
		return 0
	}
	if destPtr == 0 {
		return -1
	}
	bufPtr, err := n.allocBuffer(ctx, length)
	if err != nil {
		log.Printf("entropy alloc failed: %v", err)
		return -2
	}
	defer n.freeBuffer(ctx, bufPtr, length)
	status, err := n.callFill(ctx, bufPtr, length)
	if err != nil {
		log.Printf("entropy fill call failed: %v", err)
		return -3
	}
	if status != 0 {
		return status
	}
	data, ok := n.module.Memory().Read(bufPtr, length)
	if !ok {
		return -4
	}
	if !consumer.Memory().Write(destPtr, data) {
		return -5
	}
	return 0
}

func (n *entropyNode) callFill(ctx context.Context, ptr, length uint32) (int32, error) {
	results, err := n.fill.Call(ctx, uint64(ptr), uint64(length))
	if err != nil {
		return 0, err
	}
	if len(results) == 0 {
		return 0, fmt.Errorf("entropy fill returned no status")
	}
	return int32(uint32(results[0])), nil
}

func (n *entropyNode) allocBuffer(ctx context.Context, size uint32) (uint32, error) {
	if size == 0 {
		return 0, nil
	}
	results, err := n.alloc.Call(ctx, uint64(size))
	if err != nil {
		return 0, err
	}
	if len(results) == 0 {
		return 0, fmt.Errorf("entropy alloc returned no pointer")
	}
	return uint32(results[0]), nil
}

func (n *entropyNode) freeBuffer(ctx context.Context, ptr, size uint32) {
	if ptr == 0 || size == 0 {
		return
	}
	if _, err := n.free.Call(ctx, uint64(ptr), uint64(size)); err != nil {
		log.Printf("warning: entropy free failed: %v", err)
	}
}
