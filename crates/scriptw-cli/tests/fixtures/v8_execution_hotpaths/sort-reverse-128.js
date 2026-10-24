(function workload(){
var a=[];for(var i=128;i>0;i--)a.push(i);a.sort(function(a,b){return a-b;});var s=0;for(var i=0;i<a.length;i++)s+=(i+1)*a[i];print(s);
})();
