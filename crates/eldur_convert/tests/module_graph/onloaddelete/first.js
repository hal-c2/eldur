function show(value) {
  if (value === null) {
    return "null";
  }
  return typeof value === "function" ? value.name || "function" : typeof value + " " + String(value);
}
function said(what) {
  console.log("LC " + what + ", reads " + show(window.onload));
}
window.onload = function kept() {
  console.log("LC kept heard");
  document.getElementById("out").textContent = "kept";
};
console.log("LC delete " + delete window?.onload);
said("after delete");
window.onload = function plain() {
  console.log("LC plain heard");
  document.getElementById("out").textContent = "plain";
};
said("set after delete");
window.onload = "x";
said("string after delete");
console.log("LC delete again " + delete window["onload"]);
said("after delete again");
window.onload = "y";
console.log("LC optional reads " + show(window?.onload) + " " + show(window?.["onload"]));
console.log("LC optional delete " + delete window?.["onload"]);
said("after optional delete");
window.addEventListener("load", function () {
  said("load");
});
