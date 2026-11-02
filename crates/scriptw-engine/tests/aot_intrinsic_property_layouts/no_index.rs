#[test]
fn packed_push_observes_indexed_prototype_mutations_and_custom_chains() {
    for mode in ["", "'use strict';"] {
        super::check(&format!(
            r#"
{mode}
function assert(value) {{ if (!value) throw new Error('push prototype invariant'); }}
var a=[];
for(var i=0;i<70;i++) a.push(i);
assert(a.length===70 && a[0]===0 && a[69]===69);
a.push('x',null,undefined);
assert(a.length===73 && a[70]==='x' && a[71]===null && a[72]===undefined);

var calls=0, receiver, received;
try {{
    Object.defineProperty(Object.prototype,'2',{{set:function(v){{calls++;receiver=this;received=v;}},configurable:true}});
    var b=[0,1];
    assert(b.push(7)===3 && calls===1 && receiver===b && received===7);
    assert(!Object.prototype.hasOwnProperty.call(b,'2'));
}} finally {{ delete Object.prototype['2']; }}

var partial=[0], threw=false;
try {{
    Object.defineProperty(Object.prototype,'2',{{value:99,writable:false,configurable:true}});
    try {{ partial.push(7,8,9); }} catch(e) {{ threw=e instanceof TypeError; }}
}} finally {{ delete Object.prototype['2']; }}
assert(threw && partial.length===2 && partial[1]===7 && !Object.prototype.hasOwnProperty.call(partial,'2'));

calls=0;
try {{
    Object.defineProperty(Array.prototype,'1',{{set:function(v){{calls++;received=v;}},configurable:true}});
    var c=[4];
    assert(c.push(5)===2 && calls===1 && received===5 && !Object.prototype.hasOwnProperty.call(c,'1'));
}} finally {{ delete Array.prototype['1']; Array.prototype.length=0; }}

calls=0;
try {{
    Object.defineProperty(Object.prototype,'01',{{get:function(){{calls++;throw new Error('not an index');}},configurable:true}});
    var d=[]; d.push(1,2,3);
    assert(calls===0 && d.length===3 && d[1]===2);
}} finally {{ delete Object.prototype['01']; }}

var custom=Object.create(Array.prototype);
Object.defineProperty(custom,'0',{{set:function(v){{calls++;received=v;}},configurable:true}});
var e=[]; Object.setPrototypeOf(e,custom); calls=0;
assert(e.push(12)===1 && calls===1 && received===12 && !Object.prototype.hasOwnProperty.call(e,'0'));
var traps=0;
var proxy=new Proxy(Object.create(Array.prototype),{{set:function(target,key,value,actual){{traps++;return Reflect.set(target,key,value,actual);}}}});
var f=[]; Object.setPrototypeOf(f,proxy);
assert(f.push(21)===1 && traps===1 && f[0]===21);
print('ok');
"#
        ));
    }
}
