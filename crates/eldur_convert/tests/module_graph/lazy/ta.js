import { log } from "./log.js";
import { probe, attempt } from "./tb.js";
attempt("ta reads early before its declaration", () => early);
let early = "early";
export const K = "K";
export let n = 1;
export class Box {}
export default "D";
let late;
export function readLate() { return late; }
export function setN(v) { n = v; }
export function assignK() { K = "x"; }
log("ta runs, probe " + probe() + " " + early);
late = "set";
