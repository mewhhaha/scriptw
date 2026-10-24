(function workload(){
var a=[];for(var i=512;i>0;i--)a.push(i);var b=a.toSorted(function(a,b){return a-b;});var s=a[0];for(var i=0;i<b.length;i++)s+=(i+1)*b[i];print(s);
})();
