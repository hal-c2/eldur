var calls = 0;
function onLoad() {
  calls += 1;
  console.log("LC load fn " + calls);
  document.getElementById("out").textContent = "load " + calls;
}
window.addEventListener("load", onLoad);
window.addEventListener("load", onLoad);
var handler = {
  handleEvent: function (e) {
    console.log("LC load object " + e.type);
  },
};
window.addEventListener("load", handler);
window.addEventListener("load", handler, false);
window.addEventListener("load", onLoad, { capture: true });
window.addEventListener("load", onLoad, true);
document.addEventListener("load", function () {
  console.log("LC document load");
});
function onReady() {
  console.log("LC ready on " + (this === window ? "window" : "document"));
}
document.addEventListener("DOMContentLoaded", onReady);
window.addEventListener("DOMContentLoaded", onReady);
document.addEventListener("DOMContentLoaded", onReady);
window.addEventListener("DOMContentLoaded", onReady, true);
document.addEventListener("DOMContentLoaded", function () {
  console.log("LC ready capture on document");
}, true);
