export function log(line) { console.log("LC " + line); }
export function options(name) {
  return {
    get capture() {
      log(name + " capture");
      return false;
    },
    get once() {
      log(name + " once");
      return false;
    },
    get passive() {
      log(name + " passive");
      return false;
    },
    get signal() {
      log(name + " signal");
      return undefined;
    },
  };
}
