import { log } from "./log.js";
import "./ca.js";
export const b = "b";
log("cb runs");
document.addEventListener("DOMContentLoaded", () => log("cb dcl"));
