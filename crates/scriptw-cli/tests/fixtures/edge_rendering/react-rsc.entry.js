import React from 'react';
import {renderToReadableStream} from 'react-server-dom-webpack/server.edge';

const e = React.createElement;
function Row({id}) {
  return e('li', {'data-id':id}, 'Item <'+id+'> & café');
}
function Catalog() {
  return e('main', null, e('h1', null, 'Catalog'),
    e('ul', null, Array.from({length:32}, (_, i) => e(Row, {key:i, id:i}))));
}
async function AsyncCatalog() {
  const title = await Promise.resolve('Async catalog');
  return e('section', null, e('h1', null, title), e(Catalog));
}
globalThis.applicationRun = async function (mode) {
  let component = Catalog;
  if (mode === 'async') component = AsyncCatalog;
  const stream = renderToReadableStream(e(component), {});
  const reader = stream.getReader();
  const decoder = new TextDecoder();
  let flight = '';
  for (;;) {
    const chunk = await reader.read();
    if (chunk.done) break;
    flight += decoder.decode(chunk.value, {stream:true});
  }
  flight += decoder.decode();
  // Check real production Flight records, not an HTML/JSON stand-in renderer.
  const records = flight.trimEnd().split('\n').map(line => {
    const colon = line.indexOf(':');
    if (colon < 1) throw new Error('Missing Flight record id');
    const payload = line.slice(colon+1);
    if (payload[0] === 'E') throw new Error('Flight error record: '+payload);
    return {id:line.slice(0, colon), payload};
  });
  const root = records.find(record => record.id === '0');
  if (!root || !flight.includes('Item <31> & café')) throw new Error('Incomplete Flight model');
  return {react:React.version, mode, flight};
};
