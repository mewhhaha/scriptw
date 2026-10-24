(function workload(){
var a=["abcd"],s=0;for(var i=0;i<1024;i++)s+=a.join(",").length;print(s);
})();
