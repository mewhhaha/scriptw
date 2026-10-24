(function workload(){
function text(){return "a\ud800"+"\udc00b"+"\u0000c";}var s=0;for(var i=0;i<4096;i++)s+=text().length;print(s);
})();
