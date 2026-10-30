/**
 * Shared by computed.mjs and geometry.mjs: executes a case's `run` steps
 * with Playwright's real input, so :hover, :focus and transitions behave as
 * they do for a user.
 *
 * Steps: {"eval": js} {"click": sel} {"hover": sel} {"focus": sel}
 *        {"type": [sel, text]} {"wait": ms}
 */
export async function runSteps(page, steps) {
  for (const step of steps ?? []) {
    const [kind] = Object.keys(step);
    const arg = step[kind];
    switch (kind) {
      case "eval":
        await page.evaluate(arg);
        break;
      case "click":
        await page.click(arg);
        break;
      case "hover":
        await page.hover(arg);
        break;
      case "focus":
        await page.focus(arg);
        break;
      case "type":
        await page.click(arg[0]);
        await page.keyboard.type(arg[1]);
        break;
      case "wait":
        await page.waitForTimeout(arg);
        break;
      default:
        throw new Error(`unknown run step: ${JSON.stringify(step)}`);
    }
  }
}
