(function workload(){
var a=[];for(var i=0;i<1024;i++)a.push("ab\ud800\u0000");var s=a.join("\udfff");print(s.length+s.charCodeAt(2)+s.charCodeAt(s.length-1));
})();
