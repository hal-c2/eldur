function listen(name) {
  document.addEventListener("DOMContentLoaded", function () {
    console.log("LC ready from " + name);
  });
}
listen("a function");
if (document.getElementById("out")) {
  window.addEventListener("load", function () {
    console.log("LC load from a block");
  });
}
document.addEventListener("DOMContentLoaded", function () {
  console.log("LC ready");
  document.addEventListener("DOMContentLoaded", function () {
    console.log("LC ready added while ready fires");
  });
  window.addEventListener("load", function () {
    console.log("LC load added by ready");
    window.addEventListener("load", function () {
      console.log("LC load added while load fires");
    });
  });
  window.onload = function () {
    console.log("LC onload set by ready");
  };
});
window.addEventListener("load", function () {
  console.log("LC load");
  document.getElementById("out").textContent = "load";
});
