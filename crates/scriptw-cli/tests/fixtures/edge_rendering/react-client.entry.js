import React from 'react';
import {createRoot, hydrateRoot} from 'react-dom/client';
import {flushSync} from 'react-dom';
import {renderToString} from 'react-dom/server';

const e = React.createElement;
function App({ready}) {
  const [count, setCount] = React.useState(0);
  React.useLayoutEffect(() => {if (ready) ready();}, []);
  return e('main', null,
    e('button', {onClick:() => setCount(value => value+1)}, 'Count '+count),
    e('ul', null, Array.from({length:64}, (_, i) =>
      e('li', {key:i, 'data-id':i, 'data-count':count}, 'Item '+i+' / '+count+' & café'))));
}
function checked(container, count) {
  if (container.querySelectorAll('li').length !== 64) throw new Error('Missing rows');
  if (container.querySelector('button').textContent !== 'Count '+count) throw new Error('Wrong state');
  const rows = container.querySelectorAll('li');
  for (let i=0; i<rows.length; i++) {
    if (rows[i].textContent !== 'Item '+i+' / '+count+' & café' || rows[i].dataset.count !== String(count)) {
      throw new Error('Wrong rendered row '+i);
    }
  }
}
globalThis.applicationRun = async function (mode) {
  const container = document.createElement('div');
  document.body.append(container);
  let root;
  let before;
  let start;
  let finish;
  let count = 0;
  const errors = [];
  try {
    if (mode === 'hydrate') {
      container.innerHTML = renderToString(e(App));
      before = container.querySelector('li');
      let complete;
      const ready = new Promise(resolve => {complete=resolve;});
      start = performance.now();
      root = hydrateRoot(container, e(App, {ready:complete}), {onRecoverableError:error => errors.push(String(error))});
      await ready;
      container.getBoundingClientRect();
      finish = performance.now();
      if (before !== container.querySelector('li')) throw new Error('Hydration replaced existing DOM');
    } else if (mode === 'mount') {
      start = performance.now();
      flushSync(() => {root=createRoot(container);root.render(e(App));});
      container.getBoundingClientRect();
      finish = performance.now();
    } else if (mode === 'update') {
      flushSync(() => {root=createRoot(container);root.render(e(App));});
      checked(container, 0);
      before = container.querySelector('li');
      start = performance.now();
      flushSync(() => container.querySelector('button').dispatchEvent(new MouseEvent('click', {bubbles:true})));
      container.getBoundingClientRect();
      finish = performance.now();
      count=1;
      if (before !== container.querySelector('li')) throw new Error('Update remounted stable rows');
    } else {
      throw new Error('Unknown client operation');
    }
    checked(container, count);
    if (errors.length) throw new Error(errors.join('\n'));
    return {react:React.version, mode, rows:64, count, checked:true, elapsed_ns:Math.round((finish-start)*1e6)};
  } finally {
    if (root) flushSync(() => root.unmount());
    container.remove();
  }
};
globalThis.showApplication = function () {
  const root=createRoot(document.getElementById('preview'));
  flushSync(() => root.render(e(App)));
};
