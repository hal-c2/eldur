import { log } from "./log.js";
const shown = { toString() { throw new Error("shown"); } };
export const label = `${shown}`;
log("hb runs");
