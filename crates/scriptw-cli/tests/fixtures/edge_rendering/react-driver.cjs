'use strict';
// Upstream React executes unchanged. Host globals below are Node's real Web APIs.
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const assert = require('node:assert/strict');
const {createRequire} = require('node:module');
const {webcrypto} = require('node:crypto');
const {MessageChannel} = require('node:worker_threads');
const hash = value => require('node:crypto').createHash('sha256').update(value).digest('hex');
const read = name => fs.readFileSync(name, 'utf8');
const json = name => JSON.parse(read(name));
const save = (name, value) => fs.writeFileSync(name, JSON.stringify(value, null, 2)+'\n');
const ns = () => process.hrtime.bigint();
const elapsed = start => Number(ns()-start);
const cases = ['rsc-sync', 'rsc-async', 'web-url', 'web-json'];
const RSC_WRAPPER = '\nexport const run = globalThis.applicationRun;\nexport function encode(value) { return JSON.stringify(value); }\n';
function context() {
  return vm.createContext({console, TextEncoder, TextDecoder, ReadableStream, TransformStream,
    Request, Response, Headers, URL, AbortController, AbortSignal, Blob, FormData,
    MessageChannel, queueMicrotask, setTimeout, clearTimeout,
    performance, crypto:webcrypto, print:() => {}});
}
async function reference(directory, name) {
  const [kind, mode] = name.split('-');
  const realm = context();
  new vm.Script(read(path.join(directory, kind+'.js'))).runInContext(realm);
  return JSON.stringify(await realm.applicationRun(mode));
}
async function build(tools, sources, output) {
  const use = createRequire(path.join(tools, 'package.json'));
  for (const [name, version] of Object.entries({react:'19.3.1','react-dom':'19.3.1',
    'react-server-dom-webpack':'19.3.1',esbuild:'0.28.2',playwright:'1.64.1'})) {
    assert.equal(use(name+'/package.json').version,version,'upstream package pin: '+name);
  }
  const esbuild = use('esbuild');
  const legacy = path.join(path.dirname(use.resolve('react-dom/package.json')), 'cjs/react-dom-server-legacy.browser.production.js');
  const inputs = new Set([use.resolve('esbuild'), use.resolve('react/package.json'),
    use.resolve('react-dom/package.json'), use.resolve('react-server-dom-webpack/package.json')]);
  const variants = [['ssr','react-ssr.entry.js'], ['rsc','react-rsc.entry.js'],
    ['client','react-client.entry.js'], ['web','edge-web.entry.js']];
  for (const [name, file] of variants) {
    const options = {absWorkingDir:tools, entryPoints:[path.join(sources,file)], bundle:true,
      nodePaths:[path.join(tools,'node_modules')], format:'iife', platform:'browser', target:'esnext',
      minify:true, define:{'process.env.NODE_ENV':'"production"'}, write:false, metafile:true};
    if (name === 'rsc') options.conditions=['react-server'];
    if (name === 'ssr' || name === 'client') options.alias={'react-dom/server':legacy};
    const result = await esbuild.build(options);
    fs.writeFileSync(path.join(output,name+'.js'), result.outputFiles[0].contents);
    for (const input of Object.keys(result.metafile.inputs)) inputs.add(path.resolve(tools,input));
  }
  const ssrOutputs=[];
  for (let i=0;i<2;i++) {
    let stdout='';
    const realm=context();
    realm.print=value => {stdout+=String(value)+'\n';};
    new vm.Script(read(path.join(output,'ssr.js'))).runInContext(realm);
    ssrOutputs.push(stdout);
  }
  assert.equal(ssrOutputs[0],ssrOutputs[1], 'SSR reference must be deterministic');
  const manifest=json(path.join(sources,'portable-manifest.json'));
  for (const row of manifest.execution) fs.copyFileSync(path.join(sources,row.file),path.join(output,row.file));
  fs.copyFileSync(path.join(output,'ssr.js'),path.join(output,'react-ssr.js'));
  manifest.execution.push({file:'react-ssr.js',stdout:ssrOutputs[0],label:'React SSR',category:'react-ssr',operations:32,
    scope:'Real production React renderToString: hooks/context and32 escaped rows; no DOM or Server Components.'});
  save(path.join(output,'manifest.json'),manifest);
  const support={schema:'scriptw-react-support-v1',expected:{},inputs:{},versions:{}};
  for (const name of cases) {
    const value=await reference(output,name);
    assert.equal(value,await reference(output,name),'reference must be deterministic: '+name);
    support.expected[name]=value+'\n';
  }
  const version=use('react/package.json').version;
  support.expected['client-mount']=JSON.stringify({react:version,mode:'mount',rows:64,count:0,checked:true})+'\n';
  for (const name of [...cases,'client-mount']) {
    const [kind,mode]=name.split('-');
    const suffix='\nglobalThis.applicationRun('+JSON.stringify(mode)+').then(result=>{delete result.elapsed_ns;print(JSON.stringify(result));});\n';
    fs.writeFileSync(path.join(output,'capability-'+name+'.js'),read(path.join(output,kind+'.js'))+suffix);
  }
  for (const name of ['react','react-dom','react-server-dom-webpack','esbuild','playwright']) support.versions[name]=use(name+'/package.json').version;
  for (const input of [...inputs].sort()) support.inputs[input]=hash(fs.readFileSync(input));
  save(path.join(output,'support.json'),support);
  return support;
}
async function server(directory,name) {
  assert(cases.includes(name),'unknown server case');
  const [kind,mode]=name.split('-');
  let start=ns();
  const script=new vm.Script(read(path.join(directory,kind+'.js')));
  const parse_ns=elapsed(start);
  start=ns();
  const realm=context();
  script.runInContext(realm);
  const initialization_ns=elapsed(start);
  const expected=json(path.join(directory,'support.json')).expected[name];
  const observations=[];
  for (let index=0;index<10;index++) {
    start=ns();
    const result=await realm.applicationRun(mode);
    const elapsed_ns=elapsed(start);
    const stdout=JSON.stringify(result)+'\n';
    assert.equal(stdout,expected,'server output changed, including warmup');
    observations.push({index,warmup:index<3,elapsed_ns,stdout});
  }
  return {schema:'scriptw-react-server-v1',case:name,node:process.versions,parse_ns,initialization_ns,
    samples:7,warmups:3,observations};
}
async function retainedModule(directory,sampleCount='7',warmupCount='3',selectedCase='all') {
  const samples=Number(sampleCount);
  const warmups=Number(warmupCount);
  assert(Number.isInteger(samples)&&samples>0,'positive integer samples required');
  assert(Number.isInteger(warmups)&&warmups>=0&&samples+warmups<=1000,'at most 1000 total calls');
  assert(['all','rsc-sync','rsc-async'].includes(selectedCase),'unknown retained case');
  let caseOrder=['rsc-sync','rsc-async'];
  if(selectedCase!=='all') caseOrder=[selectedCase];
  assert.equal(typeof vm.SourceTextModule,'function',
    'run with --experimental-vm-modules on a Node version that exposes vm.SourceTextModule');
  const sourcePath=path.join(directory,'rsc.js');
  const supportPath=path.join(directory,'support.json');
  const source=read(sourcePath);
  const adapted=source+RSC_WRAPPER;
  const supportBytes=fs.readFileSync(supportPath);
  const support=JSON.parse(supportBytes);
  assert.equal(support.versions.react,'19.3.1');
  assert.equal(support.versions['react-server-dom-webpack'],'19.3.1');
  const realmStart=ns();
  const realm=context();
  const realm_creation_ns=elapsed(realmStart);
  const parseStart=ns();
  const module=new vm.SourceTextModule(adapted,{context:realm,identifier:'rsc.js'});
  const source_module_parse_ns=elapsed(parseStart);
  const initializeStart=ns();
  await module.link(() => { throw new Error('RSC bundle unexpectedly imports a dependency'); });
  await module.evaluate();
  const module_link_and_initialization_ns=elapsed(initializeStart);
  const {run,encode}=module.namespace;
  assert.equal(typeof run,'function');
  assert.equal(typeof encode,'function');
  const observations=[];
  for(const name of caseOrder) {
    const mode=name.slice(4);
    const expected=support.expected[name];
    assert.equal(typeof expected,'string','missing Node Flight expectation: '+name);
    const expectedJson=expected.replace(/\n$/,'');
    for(let index=0;index<samples+warmups;index++) {
      const start=ns();
      const result=await run(mode);
      const elapsed_ns=elapsed(start);
      const actual=encode(result);
      assert.equal(actual,expectedJson,'retained-module output changed, including warmup: '+name);
      observations.push({case:name,index,warmup:index<warmups,elapsed_ns,exact_node_output_checked:true,
        output_json_sha256:hash(actual),output_json_bytes:Buffer.byteLength(actual)});
    }
  }
  return {schema:'scriptw-react-retained-module-reference-v1',node:process.versions,
    case_order:caseOrder,case_selection:selectedCase,
    gc_policy:'normal V8 JIT and natural GC; no forced collection between calls',
    realm_policy:'one fresh context and one linked/evaluated SourceTextModule per process',
    request_clock:'await applicationRun(mode); excludes JSON encoding, exact golden validation and output checks',
    samples,warmups,realm_creation_ns,source_module_parse_ns,module_link_and_initialization_ns,
    inputs:{source:sourcePath,source_sha256:hash(source),wrapper:RSC_WRAPPER,wrapper_sha256:hash(RSC_WRAPPER),
      adapted_source_sha256:hash(adapted),support:supportPath,support_sha256:hash(supportBytes)},observations};
}
async function browser(tools,directory,chromium,index,screenshot) {
  const use=createRequire(path.join(tools,'package.json'));
  const {chromium:engine}=use('playwright');
  const http=require('node:http');
  const source=read(path.join(directory,'client.js'));
  const host=http.createServer((request,response) => {
    response.setHeader('Cross-Origin-Opener-Policy','same-origin');
    response.setHeader('Cross-Origin-Embedder-Policy','require-corp');
    if(request.url==='/client.js') {response.setHeader('Content-Type','text/javascript');response.end(source);return;}
    response.setHeader('Content-Type','text/html;charset=utf-8');
    response.end('<!doctype html><meta charset="utf-8"><title>React benchmark</title><div id="preview"></div><script src="/client.js"></script>');
  });
  await new Promise(resolve=>host.listen(0,'127.0.0.1',resolve));
  let instance;
  try {
    instance=await engine.launch({executablePath:chromium,headless:true,args:['--disable-background-networking','--disable-component-update']});
    const page=await instance.newPage({viewport:{width:1000,height:900}});
    const errors=[];
    page.on('pageerror',error=>errors.push(String(error)));
    const start=ns();
    await page.goto('http://127.0.0.1:'+host.address().port,{waitUntil:'load'});
    assert(await page.evaluate(()=>crossOriginIsolated),'high resolution clock requires isolation');
    const initialization_ns=elapsed(start);
    let order=['mount','update','hydrate'];
    const block=Math.floor(index/4);
    if(block===1) order.reverse();
    const offset=index%3;
    order=order.slice(offset).concat(order.slice(0,offset));
    const observations=[];
    const version=use('react/package.json').version;
    for (const mode of order) for(let iteration=0;iteration<10;iteration++) {
      const result=await page.evaluate(mode=>globalThis.applicationRun(mode),mode);
      const elapsed_ns=result.elapsed_ns;
      delete result.elapsed_ns;
      let count=0;
      if(mode==='update') count=1;
      assert.deepEqual(result,{react:version,mode,rows:64,count,checked:true});
      assert(Number.isSafeInteger(elapsed_ns)&&elapsed_ns>0,'positive browser entry time required');
      observations.push({mode,index:iteration,warmup:iteration<3,elapsed_ns,result});
    }
    assert.deepEqual(errors,[],'browser errors');
    if(screenshot) {
      await page.evaluate(()=>showApplication());
      await page.screenshot({path:screenshot,fullPage:true});
    }
    return {schema:'scriptw-react-browser-v1',samples:7,warmups:3,process_index:index,block,order,
      initialization_ns,browser_version:instance.version(),user_agent:await page.evaluate(()=>navigator.userAgent),
      cross_origin_isolated:true,chromium_sha256:hash(fs.readFileSync(chromium)),observations,errors};
  } finally {
    if(instance) await instance.close();
    await new Promise(resolve=>host.close(resolve));
  }
}
(async()=>{
  const [mode,...args]=process.argv.slice(2);
  let result;
  if(mode==='build') result=await build(...args);
  else if(mode==='server') result=await server(...args);
  else if(mode==='retained-module') result=await retainedModule(...args);
  else if(mode==='browser') result=await browser(args[0],args[1],args[2],Number(args[3]),args[4]);
  else throw new Error('Expected build, server or browser');
  process.stdout.write(JSON.stringify(result)+'\n',()=>process.exit(0));
})().catch(error=>{console.error(error.stack);process.exit(1);});
