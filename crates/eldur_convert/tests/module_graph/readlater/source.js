import { log } from "./log.js";
const list = [1];
list.push(2);
export const copy = [...list];
log("copy " + copy.length);
const source = {};
Object.defineProperty(source, "x", { get() { throw new Error("getter"); } });
export const { x } = source;
log("source runs");
