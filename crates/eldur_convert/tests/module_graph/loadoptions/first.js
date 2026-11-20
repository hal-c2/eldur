function options(name) {
  return {
    get capture() {
      console.log("LC " + name + " capture");
      return false;
    },
    get once() {
      console.log("LC " + name + " once");
      return false;
    },
    get passive() {
      console.log("LC " + name + " passive");
      return false;
    },
    get signal() {
      console.log("LC " + name + " signal");
      return undefined;
    },
  };
}
window.addEventListener("load", null, options("null load"));
document.addEventListener("DOMContentLoaded", null, options("null ready"));
window.addEventListener("load", function () {
  console.log("LC load");
  document.getElementById("out").textContent = "load";
}, options("load"));
var early = new AbortController();
early.abort();
window.addEventListener("load", function () {
  console.log("LC aborted load");
}, { signal: early.signal });
var later = new AbortController();
window.addEventListener("load", function () {
  console.log("LC load aborted later");
}, { signal: later.signal });
document.addEventListener("DOMContentLoaded", function () {
  console.log("LC ready aborts");
  later.abort();
});
