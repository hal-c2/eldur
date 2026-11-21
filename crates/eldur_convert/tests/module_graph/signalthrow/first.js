function said(line, run) {
  try {
    run();
    console.log("LC " + line + " added");
  } catch (e) {
    console.log("LC " + line + " threw " + e.name + " " + (e instanceof TypeError));
  }
}
var reads = {
  get capture() { console.log("LC reads capture"); return false; },
  get once() { console.log("LC reads once"); return false; },
  get passive() { console.log("LC reads passive"); return false; },
  get signal() { console.log("LC reads signal"); return 1; },
};
said("data options", function () {
  window.addEventListener("load", null, { capture: false, once: false, passive: false, signal: undefined });
});
said("data options again", function () {
  window.addEventListener("load", null, { capture: false, once: false, passive: false, signal: undefined });
});
said("getter options", function () {
  document.addEventListener("DOMContentLoaded", function () { console.log("LC getter options heard"); }, reads);
});
said("number signal", function () {
  window.addEventListener("load", function () { console.log("LC number signal heard"); }, { signal: 1 });
});
said("null signal on a null callback", function () {
  window.addEventListener("load", null, { signal: null });
});
said("getter signal", function () {
  document.addEventListener("DOMContentLoaded", function () { console.log("LC getter signal heard"); }, reads);
});
try {
  window.addEventListener("load", function () { console.log("LC top level heard"); }, { signal: "x" });
} catch (e) {
  console.log("LC top level threw " + e.name);
}
document.addEventListener("DOMContentLoaded", function () {
  said("ready string signal", function () {
    window.addEventListener("load", function () { console.log("LC ready string signal heard"); }, { signal: "x" });
  });
});
window.addEventListener("load", function () {
  console.log("LC load");
  document.getElementById("out").textContent = "load";
});
