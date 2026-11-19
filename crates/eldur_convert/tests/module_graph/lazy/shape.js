import { log } from "./log.js";
function makeBase() {
  return class { constructor(side) { this.side = side; } };
}
const Base = makeBase();
log("shape runs");
export class Square extends Base { area() { return this.side * this.side; } }
export default class extends Base { perimeter() { return 4 * this.side; } }
