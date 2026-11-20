import { log, options } from "./log.js";
document.addEventListener("click", null, options("lazy null click"));
document.addEventListener("click", () => {}, options("lazy click"));
log("lazy runs");
