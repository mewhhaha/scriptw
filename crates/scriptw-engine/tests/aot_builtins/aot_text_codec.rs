use scriptw_engine::{CompileOptions, Engine, ExecutionBackend, RealmBuilder, RunOptions};

fn assert_codec(source: &str) {
    scriptw_engine::configure_compilation_jobs(1).expect("one bounded compilation worker");
    let outcome = Engine::new(RealmBuilder::new().build())
        .run_script(
            source,
            CompileOptions::default(),
            RunOptions {
                backend: ExecutionBackend::WasmAot,
                ..RunOptions::default()
            },
        )
        .unwrap_or_else(|error| panic!("text codec failed: {error}\n{source}"));
    assert!(
        outcome.note.contains("boolean(true)"),
        "{}\n{source}",
        outcome.note
    );
}

#[test]
fn encoder_uses_unicode_scalars_and_complete_encode_into_boundaries() {
    assert_codec(
        r#"
const encoder = new TextEncoder();
const source = 'A\u00e9\u20ac\ud83d\ude00\ud800Z\udc00';
const expected = [65,195,169,226,130,172,240,159,152,128,239,191,189,90,239,191,189];
const encoded = encoder.encode(source);
if (!(encoded instanceof Uint8Array) || encoded.length !== expected.length || encoder.encoding !== 'utf-8') throw new Error('encoder shape');
for (let i=0;i<expected.length;i++) if (encoded[i] !== expected[i]) throw new Error('scalar byte ' + i);
if (!Object.getOwnPropertyDescriptor(TextEncoder.prototype,'encoding').enumerable || !Object.getOwnPropertyDescriptor(TextEncoder.prototype,'encode').enumerable) throw new Error('WebIDL descriptors');
if (encoder.encode().length !== 0 || encoder.encode(undefined).length !== 0 || encoder.encode(null).length !== 4) throw new Error('optional coercion');
for (let size=0;size<20;size++) {
  const buffer = new Uint8Array(size+4); buffer.fill(99);
  const result = encoder.encodeInto(source, buffer.subarray(2,2+size));
  if (result.written > size || result.read > source.length) throw new Error('counts');
  if (result.read === 4) throw new Error('split surrogate pair');
  if (buffer[0] !== 99 || buffer[1] !== 99 || buffer[size+2] !== 99) throw new Error('view bounds');
  const prefix = encoder.encode(source.slice(0,result.read));
  if (prefix.length !== result.written) throw new Error('counts follow UTF16');
  for (let i=0;i<prefix.length;i++) if (prefix[i] !== buffer[i+2]) throw new Error('prefix');
  if (result.written < size && buffer[result.written+2] !== 99) throw new Error('wrote partial scalar');
}
const short = encoder.encodeInto('\ud83d\ude00', new Uint8Array(3));
const enough = encoder.encodeInto('\ud83d\ude00', new Uint8Array(4));
if (short.read !== 0 || short.written !== 0 || enough.read !== 2 || enough.written !== 4) throw new Error('astral counts');
true;
"#,
    );
}

#[test]
fn decoder_streams_every_split_and_resets_after_flush() {
    assert_codec(
        r#"
const bytes = new TextEncoder().encode('\ufeffA\u00e9\u20ac\ud83d\ude00Z');
for (let split=0;split<=bytes.length;split++) {
  const decoder = new TextDecoder(' \tUTF-8\r\n');
  const first = decoder.decode(bytes.subarray(0,split), {stream:true});
  const second = decoder.decode(bytes.subarray(split), {stream:true});
  if (first+second+decoder.decode() !== 'A\u00e9\u20ac\ud83d\ude00Z') throw new Error('split ' + split);
  if (decoder.decode(bytes) !== 'A\u00e9\u20ac\ud83d\ude00Z') throw new Error('reset BOM');
}
const decoder = new TextDecoder(); let text='';
for (const byte of bytes) text += decoder.decode(new Uint8Array([byte]), {stream:true});
if (text+decoder.decode() !== 'A\u00e9\u20ac\ud83d\ude00Z') throw new Error('one-byte chunks');
for (const label of ['utf8','unicode-1-1-utf-8','unicode11utf8','unicode20utf8','x-unicode20utf8']) if (new TextDecoder(label).encoding !== 'utf-8') throw new Error('labels');
true;
"#,
    );
}

#[test]
fn decoder_recovery_fatal_and_bom_policies_are_observable() {
    assert_codec(
        r#"
const replacement='\ufffd';
const cases = [
  [[0xc0,0xaf],replacement+replacement],
  [[0xe0,0x80,0x80],replacement+replacement+replacement],
  [[0xed,0xa0,0x80],replacement+replacement+replacement],
  [[0xf4,0x90,0x80,0x80],replacement+replacement+replacement+replacement],
  [[0xe2,0x82],replacement],
  [[0xe2,0x82,65],replacement+'A'],
  [[0xf0,0x9f,0x98,0x80],'\ud83d\ude00']
];
for (const item of cases) if (new TextDecoder().decode(new Uint8Array(item[0])) !== item[1]) throw new Error('recovery '+item[0]);
const fatal = new TextDecoder(undefined,{fatal:true});
if (!fatal.fatal || fatal.ignoreBOM || new TextDecoder().fatal) throw new Error('flags');
let threw=false; try {fatal.decode(new Uint8Array([0xe2,0x82]));} catch(e) {threw=e instanceof TypeError;}
if (!threw || fatal.decode(new Uint8Array([65])) !== 'A') throw new Error('fatal flush reset');
if (fatal.decode(new Uint8Array([0xf0,0x9f]),{stream:true}) !== '') throw new Error('fatal partial');
if (fatal.decode(new Uint8Array([0x98,0x80])) !== '\ud83d\ude00') throw new Error('fatal continuation');
let queueError=false;
try {fatal.decode(new Uint8Array([0xe2,65,66]),{stream:true});}catch(error){queueError=error instanceof TypeError;}
if(!queueError||fatal.decode()!=='AB')throw new Error('fatal unread queue');
const keep=new TextDecoder(undefined,{ignoreBOM:true});
if (!keep.ignoreBOM || keep.decode(new Uint8Array([239,187,191,65])) !== '\ufeffA') throw new Error('ignoreBOM');
const strip=new TextDecoder();
if (strip.decode(new Uint8Array([65,239,187,191])) !== 'A\ufeff') throw new Error('interior BOM');
const incomplete=new TextDecoder();
if (incomplete.decode(new Uint8Array([0xe2]),{stream:true}) !== '' || incomplete.decode() !== replacement) throw new Error('final flush');
true;
"#,
    );
}

#[test]
fn decoder_accepts_buffer_source_ranges_and_codecs_enforce_brands() {
    assert_codec(
        r#"
function rejects(fn,kind) {let ok=false;try {fn();}catch(error){ok=error instanceof kind;}if(!ok)throw new Error('missing rejection');}
const encoder=new TextEncoder(); const decoder=new TextDecoder();
const buffer=new Uint8Array([88,65,66,89]).buffer;
if (decoder.decode(new DataView(buffer,1,2)) !== 'AB' || decoder.decode(new Uint16Array(buffer,0,2)) !== 'XABY' || decoder.decode(buffer) !== 'XABY') throw new Error('BufferSource range');
rejects(()=>decoder.decode([65]),TypeError); rejects(()=>decoder.decode(null),TypeError);
rejects(()=>new TextDecoder('windows-1252'),RangeError);
rejects(()=>new TextDecoder('utf-16'),RangeError);
rejects(()=>TextEncoder(),TypeError); rejects(()=>TextDecoder(),TypeError);
rejects(()=>encoder.encodeInto('A',new Int8Array(2)),TypeError);
rejects(()=>encoder.encodeInto('A',new Proxy(new Uint8Array(2),{})),TypeError);
for (const receiver of [{},TextEncoder.prototype,new Proxy(encoder,{})]) rejects(()=>TextEncoder.prototype.encode.call(receiver,'A'),TypeError);
for (const receiver of [{},TextDecoder.prototype,new Proxy(decoder,{})]) rejects(()=>TextDecoder.prototype.decode.call(receiver,new Uint8Array([65])),TypeError);
class Encoder extends TextEncoder {} class Decoder extends TextDecoder {}
if (!(new Encoder() instanceof Encoder) || new Decoder().decode(new Uint8Array([65])) !== 'A') throw new Error('subclass');
const descriptor=Object.getOwnPropertyDescriptor(TextDecoder.prototype,'fatal');
rejects(()=>descriptor.get.call({}),TypeError);
true;
"#,
    );
}

#[test]
fn dictionary_getters_follow_order_and_survive_reentrancy() {
    assert_codec(
        r#"
let log=''; const options={get fatal(){log+='f';return true;},get ignoreBOM(){log+='b';return false;}};
const decoder=new TextDecoder({toString(){log+='l';return 'UTF-8';}},options);
if(log!=='lfb')throw new Error('construction order '+log);
log=''; let invalid=false;
try { new TextDecoder({toString(){log+='l';return 'unsupported';}},options); } catch(error) {invalid=error instanceof RangeError;}
if(!invalid||log!=='lfb')throw new Error('invalid label conversion order '+log);
let touched=false;
try {decoder.decode([],{get stream(){touched=true;return false;}});}catch(error){}
if(touched)throw new Error('input admission before dictionary');
const input=new Uint8Array([65]);
const result=decoder.decode(input,{get stream(){input[0]=66; return false;}});
if(result!=='B')throw new Error('snapshot timing');
let stringConversions=0; const encoder=new TextEncoder();
const encoded=encoder.encode({toString(){stringConversions++;return '\ud800';}});
if(stringConversions!==1||encoded.length!==3)throw new Error('one ToString');
true;
"#,
    );
}
