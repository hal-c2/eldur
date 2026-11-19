import { log } from "./log.js";
window.addEventListener("load", () => log("listens load"));
document.addEventListener("DOMContentLoaded", () => log("listens DOMContentLoaded"));
log("listens runs");
