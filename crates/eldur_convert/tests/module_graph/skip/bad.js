import { log } from "./log.js";
log("bad runs");
document.addEventListener("DOMContentLoaded", () => log("bad early dcl"));
throw new Error("bad");
document.addEventListener("DOMContentLoaded", () => log("bad late dcl"));
