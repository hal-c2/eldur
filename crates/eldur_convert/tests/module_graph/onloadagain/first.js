window.addEventListener("load", function () {
  console.log("LC listener sets onload again");
  window.onload = null;
  window.onload = function () {
    console.log("LC onload set again");
    document.getElementById("out").textContent = "set again";
  };
});
window.onload = function () {
  console.log("LC first onload");
  document.getElementById("out").textContent = "first";
};
window.addEventListener("load", function () {
  console.log("LC load after");
  document.getElementById("out").textContent += " after";
});
