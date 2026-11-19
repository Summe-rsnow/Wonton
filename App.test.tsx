// The Rust root tested from TypeScript: `nsh test` runs this file in a
// headless runner it builds with the crate, and `open(".")` starts the root
// (`nsh.json` "root").
import { expect, open, test } from "nsh/test";

test("opens the add-account dialog", async () => {
  const app = await open(".");
  expect(app.texts().join("\n")).toContain("还没有账号");
  await app.press("role:button[添加账号…]");
  expect(app.texts().join("\n")).toContain("网页授权");
  await app.press("role:button[关闭]");
  expect(app.texts().join("\n")).not.toContain("网页授权");
});
