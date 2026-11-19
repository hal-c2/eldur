import { log } from "./log.js";
import { early } from "./eb.js";
const label = document.getElementById("label");
export function read() { return label.textContent; }
log("ea runs " + label.textContent + " " + early);
label.textContent = "set by ea";
