#!/usr/bin/env python3
"""Bind the actual native Duration image and selected Number/List consumers."""
import argparse,hashlib,json,tomllib
import re
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
def sha(raw):return hashlib.sha256(raw).hexdigest()
def canonical(value):return json.dumps(value,sort_keys=True,separators=(',',':')).encode()
def production(root,owner):
 return {p.relative_to(root).as_posix() for p in (root/owner).rglob('*.rs')
  if p.name not in ('tests.rs','profile_identity.rs','kernel_identity.rs') and 'tests' not in p.relative_to(root/owner).parts and not p.name.endswith('_tests.rs')}
def generate(root):
 root=root.resolve();source='crates/scriptw-intl/data/duration-cldr-47/source-inputs.json'
 inputs=json.loads((root/source).read_bytes());paths={source}
 for row in inputs['files']:
  name=Path(row['path'])
  if name.is_absolute() or '..' in name.parts or name.as_posix() in paths:raise ValueError('invalid/repeated Duration captured input')
  raw=(root/name).read_bytes()
  if sha(raw)!=row['sha256'] or len(raw)!=row['bytes']:raise ValueError('captured Duration input changed: '+str(name))
  paths.add(name.as_posix())
 paths|={'Cargo.toml','Cargo.lock','crates/scriptw-intl/Cargo.toml','scripts/generate-intl-duration-identity.py',
  'crates/scriptw-intl/src/lib.rs','crates/scriptw-intl/src/artifact_identity.rs',
  'crates/scriptw-intl/src/provider.rs', 'crates/scriptw-intl/src/provider/conformance.rs', 'crates/scriptw-intl/src/selection.rs', 'crates/scriptw-intl/src/selection/minimal_admission.rs','crates/scriptw-intl/src/protocol.rs',
  'crates/scriptw-intl/src/service_selection.rs','crates/scriptw-intl/src/selection/manifest.rs','crates/scriptw-intl/src/selection/export.rs',
  'crates/scriptw-intl/src/supported_values.rs','crates/scriptw-intl/src/duration_wire.rs',
  'crates/scriptw-intl/src/number_protocol.rs','crates/scriptw-engine/src/intl_duration_host.rs',
  'crates/scriptw-engine/src/wasm_gc_intl_host.rs','crates/scriptw-engine/src/intl_data_images.rs',
  'crates/scriptw-intl/src/image.rs','crates/scriptw-intl/src/duration_image.rs',
  'crates/scriptw-intl/src/locale_image.rs','crates/scriptw-intl/src/number_image.rs',
  'crates/scriptw-intl/src/list_image.rs','crates/scriptw-intl/src/image_build/list.rs',
  'crates/scriptw-intl/src/image_build/export_contract.rs',
  'crates/scriptw-intl/src/image_build/locale.rs','crates/scriptw-intl/build.rs',
  'crates/scriptw-intl/data/number-cldr-47/payload-manifest.json',
  'crates/scriptw-intl/src/duration_format.rs','crates/scriptw-intl/src/duration_protocol.rs',
  'crates/scriptw-intl/src/duration_format/generated/profile.json','crates/scriptw-intl/data/duration-cldr-47/report.json',
  'crates/scriptw-intl/src/number_format/profiles/fingerprint.rs','crates/scriptw-intl/src/list_format.rs',
  'crates/scriptw-intl/src/list_format/identity.rs','crates/scriptw-intl/src/number_operation.rs','crates/scriptw-intl/src/plural_protocol.rs',
  'crates/scriptw-intl/data/list-icu-2/manifest.json',
  'crates/scriptw-intl/src/identifiers.rs'}
 paths.update(['crates/scriptw-intl/src/image_build/keyword.rs', 'crates/scriptw-intl/src/provider/keyword_aliases.rs', 'crates/scriptw-intl/src/provider/keyword_aliases/generated.rs', 'crates/scriptw-intl/src/provider/keyword_aliases/identity.rs', 'crates/scriptw-intl/data/cldr-47-bcp47/manifest.json', 'crates/scriptw-intl/src/provider/language_domain.rs'])
 paths.update(['crates/scriptw-aot-wasm/src/emit.rs', 'crates/scriptw-aot-wasm/src/emit/module_assembly.rs'])
 paths.update(['crates/scriptw-intl/src/list_image/projection.rs',
               'crates/scriptw-intl/src/list_image/projection/export.rs'])
 for owner in ('duration_format','duration_wire','number_format','number_protocol','plural_rules','list_format'):
  paths|=production(root,'crates/scriptw-intl/src/'+owner)
 paths|=production(root,'crates/scriptw-intl/src/duration_image')
 paths|=production(root,'crates/scriptw-intl/src/number_image')
 # The native/shared/host source is now complete. Its runtime and AOT caller
 # admission remain separate mandatory Root checkpoints.
 lock=tomllib.loads((root/'Cargo.lock').read_text());by_name={}
 for p in lock['package']:by_name.setdefault(p['name'],[]).append(p)
 required={'icu_list':'2.0.1','icu_list_data':'2.0.0','icu_locale':'2.0.0','icu_provider':'2.0.0',
  'icu_provider_blob':'2.0.0','icu_provider_adapters':'2.0.0',
  'serde':'1.0.228','serde_json':'1.0.149','sha2':'0.10.9'}
 queue=[]
 for name,version in required.items():
  rows=[p for p in by_name[name] if p['version']==version]
  if len(rows)!=1:raise ValueError('wrong consumed package '+name)
  queue.append(rows[0])
 packages={}
 while queue:
  p=queue.pop();key=(p['name'],p['version'])
  if key in packages:continue
  if p.get('source')!='registry+https://github.com/rust-lang/crates.io-index' or len(p.get('checksum',''))!=64:raise ValueError('unlocked registry identity')
  packages[key]={k:p[k] for k in ('name','version','source','checksum')}
  for dep in p.get('dependencies',[]):
   words=dep.split();rows=by_name[words[0]]
   if len(words)>1 and words[1][0].isdigit():rows=[r for r in rows if r['version']==words[1]]
   if len(rows)!=1:raise ValueError('ambiguous dependency '+dep)
   queue.append(rows[0])
 rows=[]
 for name in sorted(paths):
  raw=(root/name).read_bytes();rows.append({'path':name,'bytes':len(raw),'sha256':sha(raw)})
 recipe={'schema_version':1,'service':'DurationFormat','foundation_only':False,'host_call_abi':int(re.fullmatch(r'.*?pub const INTL_HOST_CALL_ABI_VERSION: u16 = (\d+);.*', (root / 'crates/scriptw-intl/src/lib.rs').read_text(), re.S).group(1)),'global_operations':[36,37,38],
  'files':rows,'packages':[packages[k] for k in sorted(packages)],'scope':'actual immutable native Duration image, retained selected Number/List owners, global/native/host source, complete primitive codecs and primary CLDR47 authority; native and AOT execution pending'}
 digest=sha(canonical(recipe));profile=sha((root/'crates/scriptw-intl/src/duration_format/generated/profile.json').read_bytes())
 rust='// Generated by scripts/generate-intl-duration-identity.py; do not edit.\n'
 for constant,value in [('DURATION_FORMAT_DATA_SHA256',digest),('DURATION_PROFILE_SHA256',profile)]:
  rust+='pub(crate) const '+constant+': [u8; 32] = [\n'
  for start in (0,32):rust+='    '+', '.join('0x'+value[i:i+2] for i in range(start,start+32,2))+',\n'
  rust+='];\n'
 return {'crates/scriptw-intl/src/duration_format/profile_identity.rs':rust.encode(),
  'crates/scriptw-intl/data/duration-cldr-47/kernel-manifest.json':(json.dumps(dict(recipe,provider_data_sha256=digest),indent=2,sort_keys=True)+'\n').encode()}
def main():
 p=argparse.ArgumentParser();p.add_argument('--repository',type=Path,default=ROOT);p.add_argument('--check',action='store_true');a=p.parse_args()
 outputs=generate(a.repository)
 for name,raw in outputs.items():
  path=a.repository/name
  if a.check:
   if path.read_bytes()!=raw:raise ValueError('stale Duration identity '+name)
  else:path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(raw)
 manifest=json.loads(next(raw for name,raw in outputs.items() if name.endswith('.json')))
 print(json.dumps({'source_only':True,'sha256':manifest['provider_data_sha256'],'files':len(manifest['files']),'packages':len(manifest['packages'])}))
if __name__=='__main__':main()
