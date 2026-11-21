function said(what) {
  console.log("LC " + what + " " + (window.onload === null ? "null" : typeof window.onload));
}
window.onload = "x";
said("string");
window.onload = 5;
said("number");
window.onload = true;
said("boolean");
window.onload = undefined;
said("undefined");
var object = {
  handleEvent: function () {
    console.log("LC object heard");
  },
};
window.onload = object;
said("object kept " + (window.onload === object));
window.onload = function () {
  console.log("LC first onload");
};
window.onload = "x";
said("string after a handler");
function later() {
  window.onload = function () {
    console.log("LC onload set in a function");
  };
  window.onload = 7;
  said("number in a function");
}
later();
window.addEventListener("load", function () {
  window.onload = "y";
  said("string while load fires");
});
window.onload = function () {
  console.log("LC onload taken out while load fires");
  document.getElementById("out").textContent = "onload";
};
window.addEventListener("load", function () {
  said("load after");
  document.getElementById("out").textContent = "after";
});
