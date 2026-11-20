import { log, options } from "./log.js";
window.addEventListener("load", null, options("module null load"));
log("module runs");
import("./lazy.js");
