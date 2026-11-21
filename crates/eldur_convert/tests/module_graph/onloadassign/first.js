function show(value) {
  if (value === null) {
    return "null";
  }
  return typeof value === "function" ? value.name || "function" : typeof value + " " + String(value);
}
function said(what, value) {
  console.log("LC " + what + " " + show(value) + ", reads " + show(window.onload));
}
said("or", (window.onload ||= function orHandler() {}));
console.log("LC optional reads " + show(window?.onload) + " " + show(window?.["onload"]));
said("or again", (window.onload ||= "x"));
said("and", (window.onload &&= function andHandler() {}));
said("nullish", (window.onload ??= "y"));
window.onload = null;
said("nullish on null", (window.onload ??= "z"));
said("plus", (window.onload += ""));
said("and on null", (window.onload &&= "w"));
window.onload = function counted() {};
said("increment", window.onload++);
[window.onload] = [function fromArray() {}];
said("array pattern", undefined);
({ at: window.onload } = { at: 5 });
said("object pattern", undefined);
for (window.onload of [function loopFirst() {}, function loopLast() {}]) {
  said("loop", undefined);
}
function later() {
  said("or in a function", (window.onload ||= "q"));
  window.onload = null;
  said("or on null in a function", (window.onload ||= function lateHandler() {
    console.log("LC late handler heard");
    document.getElementById("out").textContent = "late";
  }));
  console.log("LC optional reads in a function " + show(window?.onload) + " " + show(window?.["onload"]));
}
later();
window.addEventListener("load", function () {
  said("load", undefined);
});
