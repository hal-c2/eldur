let ready = function () {
  console.log("LC ready a");
};
document.addEventListener("DOMContentLoaded", ready);
ready = function () {
  console.log("LC ready b");
};
let loaded = function () {
  console.log("LC load a");
  document.getElementById("out").textContent = "a";
};
window.addEventListener("load", loaded);
loaded = function () {
  console.log("LC load b");
  document.getElementById("out").textContent = "b";
};
