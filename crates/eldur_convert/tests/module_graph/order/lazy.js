import { log } from "./log.js";
import { s } from "./shared.js";
import { only } from "./lazydep.js";
log("lazy " + only);
export const word = "chunk";
export function shared() { return s; }
