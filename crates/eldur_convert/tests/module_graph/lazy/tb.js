import { log } from "./log.js";
import D, { K, n, Box, readLate, setN, destructure } from "./ta.js";
export function attempt(label, f) {
  try {
    f();
    log(label + " ok");
  } catch (e) {
    log(label + " " + e.name);
  }
}
log("tb runs before ta, readLate is a " + typeof readLate);
attempt("tb reads a const", () => K);
attempt("tb reads a let", () => n);
attempt("tb reads typeof a const", () => typeof K);
attempt("tb reads a class", () => new Box());
attempt("tb reads the default", () => D);
attempt("tb calls a function reading an uninitialized let", () => readLate());
attempt("tb calls a function writing an uninitialized let", () => setN(2));
attempt("tb writes an import", () => { K = 1; });
attempt("tb calls a function destructuring into an uninitialized let", () => destructure(3));
attempt("tb destructures into an import", () => { [K] = [1]; });
export function probe() { return K; }
