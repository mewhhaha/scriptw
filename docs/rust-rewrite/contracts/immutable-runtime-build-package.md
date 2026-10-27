# Immutable runtime build package

The Cargo runtime-image producer obtains R exclusively through
`build_runtime_artifact_with_source_compilation_profile`. That constructor
emits R, encodes its package, fully decodes every Wasm body and the complete
typed layout, then requires exact key/layout parity before returning. The
producer compiles that returned Wasm and publishes its exact package alongside
a source/target/configuration/package/native manifest. The leaf image crate
embeds those package bytes in an immutable target array.

Engine's private `embedded_runtime::inputs` may therefore reuse that full body
validation through `unsafe with_immutable_build_package`. Its safety contract
requires exact successful producer output and immutable static bytes. Static
lifetime alone supplies no validation proof. No external slice, CLI path,
cache entry, re-encoded envelope or native image enters this boundary.

At use, canonical admission still checks the current Intl selection, source
and compilation profile, package framing and checksum, exact payload lengths,
pool boundaries, Wasm sections/types/imports/exports, Intl component frames,
core data sections, source-profile marker and reconstructed typed layout.
Component Wasm modules are rejected. Only the repeated
function-body pass uses the producer's prior result. Native-image admission
independently retains its manifest/digest/configuration checks and Wasmtime
compatibility checks. Runtime keys and serialized artifact formats are unchanged.

The safe `with_build_package` setter always restores full body validation,
including when replacing an earlier trusted candidate. All disk-cache paths
also validate every body. Rejected candidates use the ordinary checked cache
or emitter. Engine supplies a separate immutable candidate for each of NativeHost
and Portable compilation. Each profile retains its exact raw R key, package and
native manifest; one profile cannot substitute for the other. Unsupported/custom
identities still reject the canonical package and follow normal emission.
Candidate construction remains lazy for scalar programs.

Verification must compare the producer package's fully checked and trusted
admission results by exact Wasm bytes, runtime key and layout, and retain all
existing corruption controls. A replacement control re-signs an envelope
containing an invalid function opcode and requires the safe setter to reject
it after an earlier trusted candidate. Original cold deadlines, complete
affected scopes and fresh broad suites remain mandatory; this contract grants
no acceptance by itself.

## Packed native-image source

The leaf's private image selector chooses the exact immutable Cargo packed
array. Reading that array through its mapped address can make the compressed
payload resident in addition to the decoded native image. Linux first attempts
the existing executable-file source, requiring the complete symbol range in one
read-only private mapping with matching device, inode and bounded file offset.

Some local file views report different device IDs for the opened executable and
its mapping. A private `KernelMainElf` owner supplies an independent origin proof
for the actual main executable. Only its constructor opens `/proc/self/exe`;
parsed layouts and digests cannot construct that owner. It reads at most 4 KiB
of auxiliary-vector facts and 1,024 ELF64 program headers of 56 bytes each.
The file's program-header table must equal the bounded table read at the
kernel's `AT_PHDR` address through `/proc/self/mem`. Machine, entry, header count,
header size, exact unique `PT_PHDR` and checked load bias must agree.

The header table and complete selected symbol must each have one unambiguous
read-only file-backed load owner. Writable/executable payloads, BSS tails,
overlapping loads and arithmetic overflows reject. The actual read-only mapping
must still match the executable inode and the ELF-derived file offset. The
reader's bound is exactly the immutable array's length. Proc access failures,
unsupported layouts and ambiguous ranges retain the existing embedded source.

This proof selects byte storage only. The decoded native digest, Cargo manifest,
source/profile/runtime key and destination Engine compatibility remain separate
admission requirements. Existing `SCRIPTW_WASM_TRACE` output reports the selected
source kind without payload bytes or addresses.

The 2026-10-27 source-reader extension and its malformed-header, mapping, range
and actual-source controls are implemented; whole-batch verification is pending.
