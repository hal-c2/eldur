try {
  document.addEventListener("click", () => {}, { signal: {} });
  console.log("LC lazy added");
} catch (e) {
  console.log("LC lazy threw " + e.name + " " + (e instanceof TypeError));
}
console.log("LC lazy runs");
