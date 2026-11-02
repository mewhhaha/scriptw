import React from 'react';
import {renderToString} from 'react-dom/server';

const e = React.createElement;
const Theme = React.createContext('light');
function Catalog() {
  const [items] = React.useState(() => Array.from({length:32}, (_, i) => ({id:i, title:'Item <'+i+'> & café'})));
  const theme = React.useContext(Theme);
  return e('main', {className:theme}, e('h1', null, 'Catalog'),
    e('ul', null, items.map(item => e('li', {key:item.id, 'data-id':item.id}, item.title))));
}
globalThis.applicationRun = function () {
  return {react:React.version, rows:32, html:renderToString(e(Theme.Provider, {value:'dark'}, e(Catalog)))};
};
print(JSON.stringify(globalThis.applicationRun()));
